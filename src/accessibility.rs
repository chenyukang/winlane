use crate::window_server::Inventory;
use core_foundation::array::CFArray;
use core_foundation::base::{CFGetTypeID, CFHash, CFType, CFTypeID, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::data::{CFData, CFDataRef};
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, c_void};
use std::hash::{Hash, Hasher};
use std::ptr;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use winlane::discovery::{
    READ_TIMEOUT, finish_application_scan, read_published_windows, read_with_retry,
    remote_window_token, switchable_window,
};
use winlane::search::WindowInfo;
use winlane::{tr, trf};

type AxError = i32;
type AxRef = CFTypeRef;

const AX_SUCCESS: AxError = 0;
const AX_ATTRIBUTE_UNSUPPORTED: AxError = -25205;
const AX_NOT_IMPLEMENTED: AxError = -25208;
const AX_NO_VALUE: AxError = -25212;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> u8;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXUIElementGetTypeID() -> CFTypeID;
    fn AXUIElementCreateApplication(pid: i32) -> AxRef;
    fn AXUIElementSetMessagingTimeout(element: AxRef, timeout: f32) -> AxError;
    fn AXUIElementCopyAttributeValue(
        element: AxRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AxError;
    fn AXUIElementIsAttributeSettable(
        element: AxRef,
        attribute: CFStringRef,
        settable: *mut u8,
    ) -> AxError;
    fn AXUIElementSetAttributeValue(
        element: AxRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> AxError;
    fn AXUIElementPerformAction(element: AxRef, action: CFStringRef) -> AxError;
}

type CreateRemoteElement = unsafe extern "C" fn(CFDataRef) -> AxRef;
type GetWindowId = unsafe extern "C" fn(AxRef, *mut u32) -> AxError;

struct WindowApi {
    create: CreateRemoteElement,
    window_id: GetWindowId,
}

impl WindowApi {
    fn get() -> Option<&'static Self> {
        static API: OnceLock<Option<WindowApi>> = OnceLock::new();
        API.get_or_init(|| {
            unsafe extern "C" {
                fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
            }
            // SAFETY: Darwin's RTLD_DEFAULT searches already loaded frameworks.
            // Both optional symbols have the declared C signatures. Absence is
            // supported: published AX windows remain available without them.
            unsafe {
                let create = dlsym(
                    (-2isize) as *mut c_void,
                    c"_AXUIElementCreateWithRemoteToken".as_ptr(),
                );
                let window_id = dlsym((-2isize) as *mut c_void, c"_AXUIElementGetWindow".as_ptr());
                if create.is_null() || window_id.is_null() {
                    return None;
                }
                Some(Self {
                    create: std::mem::transmute::<*mut c_void, CreateRemoteElement>(create),
                    window_id: std::mem::transmute::<*mut c_void, GetWindowId>(window_id),
                })
            }
        })
        .as_ref()
    }
}

#[derive(Clone, Default)]
struct RemoteScan {
    targets: HashSet<u32>,
    next: u64,
    elements: HashMap<u32, u64>,
}

fn remote_scans() -> &'static Mutex<HashMap<i32, RemoteScan>> {
    static SCANS: OnceLock<Mutex<HashMap<i32, RemoteScan>>> = OnceLock::new();
    SCANS.get_or_init(Mutex::default)
}

#[derive(PartialEq)]
struct Element(CFType);

impl Element {
    fn application(pid: i32) -> Option<Self> {
        if pid <= 0 {
            return None;
        }
        // SAFETY: This creates a retained AX object for the supplied process ID.
        let raw = unsafe { AXUIElementCreateApplication(pid) };
        if raw.is_null() {
            return None;
        }
        // SAFETY: The non-null Create result transfers one retain to this owner.
        let element = Self(unsafe { CFType::wrap_under_create_rule(raw) });
        element.set_timeout(READ_TIMEOUT);
        Some(element)
    }

    fn raw(&self) -> AxRef {
        self.0.as_CFTypeRef()
    }

    fn from_remote_id(pid: i32, id: u64) -> Option<Self> {
        let api = WindowApi::get()?;
        let data = CFData::from_buffer(&remote_window_token(pid, id));
        // SAFETY: The token is a valid, owned CFData with the framework's
        // process/element encoding. Accessibility authorization still applies.
        let raw = unsafe { (api.create)(data.as_concrete_TypeRef()) };
        if raw.is_null() {
            return None;
        }
        // SAFETY: The Create function returns one retained AX reference.
        let element = Self(unsafe { CFType::wrap_under_create_rule(raw) });
        element.set_timeout(0.015);
        Some(element)
    }

    fn server_id(&self) -> Option<u32> {
        let api = WindowApi::get()?;
        let mut id = 0;
        // SAFETY: self owns an AX reference; id is a valid window-ID out-pointer.
        let status = unsafe { (api.window_id)(self.raw(), &mut id) };
        (status == AX_SUCCESS && id != 0).then_some(id)
    }

    fn element(&self, name: &str) -> Result<Self, AxError> {
        let value = self.attribute(name)?;
        // SAFETY: The attribute owns a CF object whose concrete type is checked.
        if unsafe { CFGetTypeID(value.as_CFTypeRef()) != AXUIElementGetTypeID() } {
            return Err(AX_NO_VALUE);
        }
        let element = Self(value);
        element.set_timeout(READ_TIMEOUT);
        Ok(element)
    }

    fn set_timeout(&self, timeout: f32) {
        // SAFETY: self owns a valid AX object. AX timeouts are per object, so
        // window handles need this independently of their application handle.
        unsafe { AXUIElementSetMessagingTimeout(self.raw(), timeout) };
    }

    fn attribute(&self, name: &str) -> Result<CFType, AxError> {
        let result = read_with_retry(|timeout| {
            self.set_timeout(timeout);
            self.attribute_once(name)
        });
        self.set_timeout(READ_TIMEOUT);
        if std::env::var_os("WINDOWLANE_DEBUG_WINDOWS").is_some()
            && let Err(code) = &result
        {
            eprintln!("AX attribute {name}: error {code}");
        }
        result
    }

    fn attribute_once(&self, name: &str) -> Result<CFType, AxError> {
        let name = CFString::new(name);
        let mut value = ptr::null();
        // SAFETY: Both inputs remain owned through the call, and value is a
        // valid out-pointer. Copy returns a retained Core Foundation object.
        let status = unsafe {
            AXUIElementCopyAttributeValue(self.raw(), name.as_concrete_TypeRef(), &mut value)
        };
        let owned = if value.is_null() {
            None
        } else {
            // SAFETY: A non-null Copy out-value carries one retain for the caller.
            Some(unsafe { CFType::wrap_under_create_rule(value) })
        };
        if status == AX_SUCCESS {
            owned.ok_or(AX_NO_VALUE)
        } else {
            Err(status)
        }
    }

    fn string(&self, name: &str) -> Option<String> {
        self.attribute(name)
            .ok()?
            .downcast_into::<CFString>()
            .map(|value| value.to_string())
    }

    fn boolean(&self, name: &str) -> Option<bool> {
        self.attribute(name)
            .ok()?
            .downcast_into::<CFBoolean>()
            .map(bool::from)
    }

    fn elements(&self, name: &str) -> Result<Vec<Self>, AxError> {
        let value = self.attribute(name)?;
        let array = value.downcast_into::<CFArray>().ok_or(AX_NO_VALUE)?;
        let mut windows = Vec::new();
        for entry in array.iter() {
            let raw = *entry;
            // SAFETY: AXWindows contains CF objects owned by array. Check each
            // object's runtime type before treating it as an AXUIElement.
            if raw.is_null() || unsafe { CFGetTypeID(raw) != AXUIElementGetTypeID() } {
                continue;
            }
            // SAFETY: The array owns raw; Get-rule wrapping retains the element
            // so it stays alive after the temporary array is dropped.
            let window = Self(unsafe { CFType::wrap_under_get_rule(raw) });
            window.set_timeout(READ_TIMEOUT);
            windows.push(window);
        }
        Ok(windows)
    }

    fn windows(&self) -> Result<Vec<Self>, AxError> {
        // Reading the application role lets Electron enable native accessibility
        // without turning on its more expensive screen-reader mode.
        let _ = self.attribute("AXRole");
        read_published_windows(|name| {
            let elements = match name {
                "AXWindows" | "AXChildren" => self.elements(name),
                _ => self.element(name).map(|element| vec![element]),
            };
            elements.map(|elements| elements.into_iter().filter(Element::is_window).collect())
        })
    }

    fn id(&self, pid: i32) -> u64 {
        let mut hasher = DefaultHasher::new();
        pid.hash(&mut hasher);
        // SAFETY: self owns a valid CF object. AX equality/hash identifies the
        // remote element even when re-enumeration produces a new local pointer.
        unsafe { CFHash(self.raw()) }.hash(&mut hasher);
        hasher.finish() & !(1 << 63)
    }

    fn is_window(&self) -> bool {
        let role = self.string("AXRole");
        let subrole = self.string("AXSubrole");
        if std::env::var_os("WINDOWLANE_DEBUG_WINDOWS").is_some() {
            eprintln!("AX candidate role={role:?} subrole={subrole:?}");
        }
        switchable_window(role.as_deref(), subrole.as_deref())
    }

    fn set_boolean_if_supported(&self, name: &str, value: bool) -> Result<bool, AxError> {
        let name = CFString::new(name);
        let mut settable = 0;
        // SAFETY: Both inputs are live and settable is a valid Boolean out-pointer.
        let status = unsafe {
            AXUIElementIsAttributeSettable(self.raw(), name.as_concrete_TypeRef(), &mut settable)
        };
        if matches!(
            status,
            AX_ATTRIBUTE_UNSUPPORTED | AX_NOT_IMPLEMENTED | AX_NO_VALUE
        ) || (status == AX_SUCCESS && settable == 0)
        {
            return Ok(false);
        }
        if status != AX_SUCCESS {
            return Err(status);
        }
        let value = CFBoolean::from(value);
        // SAFETY: The AX element, attribute string, and Boolean remain owned for
        // this synchronous call; Set does not transfer their ownership.
        let status = unsafe {
            AXUIElementSetAttributeValue(
                self.raw(),
                name.as_concrete_TypeRef(),
                value.as_CFTypeRef(),
            )
        };
        if status == AX_SUCCESS {
            Ok(true)
        } else {
            Err(status)
        }
    }
}

pub fn is_trusted() -> bool {
    // SAFETY: This read-only status API has no arguments and never prompts.
    unsafe { AXIsProcessTrusted() != 0 }
}

pub fn request_permission() {
    // SAFETY: This is the immutable framework option key. The dictionary stays
    // alive through the call; macOS asks the user and never grants access here.
    let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
    let options = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) };
}

fn all_windows(application: &Element, pid: i32, inventory: &Inventory) -> Vec<Element> {
    complete_windows(application.windows().unwrap_or_default(), pid, inventory)
}

fn complete_windows(published: Vec<Element>, pid: i32, inventory: &Inventory) -> Vec<Element> {
    let mut seen = HashSet::new();
    let mut windows: Vec<_> = published
        .into_iter()
        .filter(|window| {
            let Some(id) = window.server_id() else {
                return true;
            };
            if inventory.rejects(pid, id) && window.boolean("AXMinimized") != Some(true) {
                return false;
            }
            seen.insert(id)
        })
        .collect();
    let mut missing = inventory.normal.get(&pid).cloned().unwrap_or_default();
    missing.retain(|id| !seen.contains(id));
    if missing.is_empty() || WindowApi::get().is_none() {
        return windows;
    }

    let mut scan = remote_scans()
        .lock()
        .unwrap()
        .get(&pid)
        .cloned()
        .unwrap_or_default();
    // A Space change can hide a previously published window without changing
    // the WindowServer inventory. Its AX ID may be behind the scan cursor.
    if scan.targets != missing {
        scan.next = 0;
        scan.targets = missing.clone();
    }
    scan.elements.retain(|id, _| {
        inventory
            .normal
            .get(&pid)
            .is_some_and(|ids| ids.contains(id))
    });
    let mut invalid_cached_element = false;
    scan.elements.retain(|&server_id, element_id| {
        if !missing.contains(&server_id) {
            return true;
        }
        if let Some(window) = Element::from_remote_id(pid, *element_id)
            && window.server_id() == Some(server_id)
            && window.is_window()
        {
            missing.remove(&server_id);
            windows.push(window);
            true
        } else {
            invalid_cached_element = true;
            false
        }
    });
    if invalid_cached_element {
        scan.next = 0;
    }
    let started = Instant::now();
    while !missing.is_empty() && started.elapsed() < Duration::from_millis(250) {
        let element_id = scan.next;
        scan.next = scan.next.wrapping_add(1);
        if let Some(window) = Element::from_remote_id(pid, element_id)
            && let Some(server_id) = window.server_id()
            && missing.contains(&server_id)
            && window.is_window()
        {
            // Electron can expose an AXUnknown object before the real AXWindow
            // for the same WindowServer ID. Only a validated window resolves it.
            missing.remove(&server_id);
            scan.elements.insert(server_id, element_id);
            windows.push(window);
        }
    }
    if std::env::var_os("WINDOWLANE_DEBUG_WINDOWS").is_some() {
        eprintln!(
            "AX other-spaces pid={pid} found={} unresolved={} next={}",
            scan.elements.len(),
            missing.len(),
            scan.next
        );
    }
    remote_scans().lock().unwrap().insert(pid, scan);
    windows
}

fn scan_application(
    pid: i32,
    app: &str,
    inventory: &Inventory,
) -> Result<Vec<WindowInfo>, AxError> {
    let application = Element::application(pid).ok_or(AX_NO_VALUE)?;
    let windows = all_windows(&application, pid, inventory);
    if std::env::var_os("WINDOWLANE_DEBUG_WINDOWS").is_some() {
        eprintln!("AX scan pid={pid} app={app:?} accepted={}", windows.len());
    }
    Ok(windows
        .into_iter()
        .map(|window| WindowInfo {
            id: window.id(pid),
            pid,
            app: app.into(),
            title: window.string("AXTitle").unwrap_or_default(),
            minimized: window.boolean("AXMinimized").unwrap_or(false),
        })
        .collect())
}

pub fn list_windows(apps: &[(i32, String)]) -> Vec<WindowInfo> {
    if !is_trusted() {
        return Vec::new();
    }
    let inventory = Inventory::read();
    remote_scans()
        .lock()
        .unwrap()
        .retain(|pid, _| apps.iter().any(|(app_pid, _)| app_pid == pid));
    // Each worker owns its AX handles; a slow app does not delay every other app.
    std::thread::scope(|scope| {
        let workers = apps
            .chunks(apps.len().div_ceil(4).max(1))
            .map(|chunk| {
                let inventory = &inventory;
                scope.spawn(move || {
                    chunk
                        .iter()
                        .flat_map(|(pid, app)| {
                            let windows = scan_application(*pid, app, inventory);
                            finish_application_scan(windows)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("window scan worker panicked"))
            .collect()
    })
}

fn find_window(pid: i32, id: u64) -> Result<Element, String> {
    if !is_trusted() {
        return Err(tr!(
            "尚未开启辅助功能权限。",
            "Accessibility access has not been granted."
        )
        .to_owned());
    }
    let application = Element::application(pid).ok_or(tr!(
        "应用已退出，请刷新窗口列表。",
        "The app has quit. Refresh the window list."
    ))?;
    let windows = all_windows(&application, pid, &Inventory::read());
    let mut matches = windows.iter().filter(|window| window.id(pid) == id);
    let window = matches.next().ok_or(tr!(
        "窗口已关闭，请刷新窗口列表。",
        "The window has closed. Refresh the window list."
    ))?;
    if matches.any(|other| other.0 != window.0) {
        return Err(tr!(
            "无法确定目标窗口，请刷新窗口列表。",
            "Could not identify the window. Refresh the window list."
        )
        .to_owned());
    }
    Ok(Element(window.0.clone()))
}

pub fn set_minimized(pid: i32, id: u64, minimized: bool) -> Result<(), String> {
    let window = find_window(pid, id)?;
    match window.set_boolean_if_supported("AXMinimized", minimized) {
        Ok(true) => Ok(()),
        Ok(false) => Err(tr!(
            "此应用不支持更改该窗口的最小化状态。",
            "This app does not support minimizing or restoring this window."
        )
        .into()),
        Err(code) => Err(trf!(
            "无法更改最小化状态（错误 {code}）。",
            "Could not change minimized state (error {code})."
        )),
    }
}

pub fn raise_window(pid: i32, id: u64) -> Result<(), String> {
    let window = find_window(pid, id)?;
    if window.boolean("AXMinimized") == Some(true) {
        match window.set_boolean_if_supported("AXMinimized", false) {
            Ok(true) => {}
            Ok(false) => {
                return Err(tr!(
                    "此应用不支持恢复该最小化窗口。",
                    "This app does not support restoring this minimized window."
                )
                .into());
            }
            Err(code) => {
                return Err(trf!(
                    "无法恢复最小化窗口（错误 {code}）。",
                    "Could not restore the window (error {code})."
                ));
            }
        }
    }
    for attribute in ["AXMain", "AXFocused"] {
        window
            .set_boolean_if_supported(attribute, true)
            .map_err(|code| {
                trf!(
                    "无法聚焦窗口（错误 {code}）。",
                    "Could not focus the window (error {code})."
                )
            })?;
    }
    let action = CFString::new("AXRaise");
    // SAFETY: The retained AX window and CFString live through the synchronous
    // action request. No ownership is transferred.
    let status = unsafe { AXUIElementPerformAction(window.raw(), action.as_concrete_TypeRef()) };
    if status == AX_SUCCESS {
        Ok(())
    } else {
        Err(trf!(
            "无法置前窗口（错误 {status}）。",
            "Could not raise the window (error {status})."
        ))
    }
}
