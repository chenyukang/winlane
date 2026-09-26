use crate::macos::platform::logging::{Level, record};
use crate::macos::platform::window_server::Inventory;
use core_foundation::array::CFArray;
use core_foundation::base::{
    CFGetTypeID, CFHash, CFRelease, CFRetain, CFType, CFTypeID, CFTypeRef, TCFType,
};
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
use winlane::core::discovery::{
    AX_NO_VALUE, FocusRead, READ_TIMEOUT, read_focused_window, read_published_windows,
    read_with_retry, remote_window_token, switchable_window_role,
};
use winlane::core::search::WindowInfo;
use winlane::{tr, trf};

pub mod auto_appclose;

type AxError = i32;
type AxRef = CFTypeRef;

const AX_SUCCESS: AxError = 0;
const AX_ATTRIBUTE_UNSUPPORTED: AxError = -25205;
const AX_ACTION_UNSUPPORTED: AxError = -25206;
const AX_NOT_IMPLEMENTED: AxError = -25208;

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

/// Last-known data of a window that disappeared from accessibility queries.
struct RememberedWindow {
    /// Winlane's stable window id, computed from the AX element identity.
    id: u64,
    title: String,
    minimized: bool,
    /// Retained `AXUIElement` address; the element usually keeps answering
    /// reads and actions after the window is minimized.
    element: usize,
}

impl Drop for RememberedWindow {
    fn drop(&mut self) {
        // SAFETY: `element` was retained when this entry was created.
        unsafe { CFRelease(self.element as CFTypeRef) };
    }
}

/// Windows seen in earlier scans, keyed by (pid, WindowServer id). Minimized
/// windows become invisible to cross-process accessibility queries while
/// their WindowServer surfaces stay in the inventory, so their element handle
/// and last-known data are kept here to stay selectable. Kept apart from
/// `RemoteScan` because the discovery scan must not hold a global lock.
fn remembered_windows() -> &'static Mutex<HashMap<i32, HashMap<u32, RememberedWindow>>> {
    static REMEMBERED: OnceLock<Mutex<HashMap<i32, HashMap<u32, RememberedWindow>>>> =
        OnceLock::new();
    REMEMBERED.get_or_init(Mutex::default)
}

#[derive(PartialEq)]
struct Element(CFType);

pub(super) fn element_id(pid: i32, element: &CFType) -> u64 {
    let mut hasher = DefaultHasher::new();
    pid.hash(&mut hasher);
    // SAFETY: CFHash reads a live, retained AX object's remote identity.
    unsafe { CFHash(element.as_CFTypeRef()) }.hash(&mut hasher);
    hasher.finish() & !(1 << 63)
}

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
        if let Err(code) = &result {
            record(Level::Debug, "accessibility", "attribute-error", || {
                format!("attribute={name} code={code}")
            });
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

    fn windows(&self, minimized_ok: bool) -> Result<Vec<Self>, AxError> {
        // Reading the application role lets Electron enable native accessibility
        // without turning on its more expensive screen-reader mode.
        let _ = self.attribute("AXRole");
        read_published_windows(|name| {
            let elements = match name {
                "AXWindows" | "AXChildren" => self.elements(name),
                _ => self.element(name).map(|element| vec![element]),
            };
            elements.map(|elements| {
                elements
                    .into_iter()
                    .filter(|e| e.switchable(minimized_ok))
                    .collect()
            })
        })
    }

    fn id(&self, pid: i32) -> u64 {
        element_id(pid, &self.0)
    }

    /// Whether this element is a window worth listing. With `minimized_ok`,
    /// minimized windows are accepted even when they report `AXDialog`.
    fn switchable(&self, minimized_ok: bool) -> bool {
        let role = self.string("AXRole");
        let subrole = self.string("AXSubrole");
        // Minimized AppKit windows report `AXDialog`; only accept that
        // subrole when the window is actually minimized.
        let minimized = if minimized_ok
            && role.as_deref() == Some("AXWindow")
            && subrole.as_deref() == Some("AXDialog")
        {
            self.boolean("AXMinimized")
        } else {
            None
        };
        record(Level::Debug, "accessibility", "candidate", || {
            format!("role={role:?} subrole={subrole:?}")
        });
        switchable_window_role(role.as_deref(), subrole.as_deref(), minimized)
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

fn all_windows(
    application: &Element,
    pid: i32,
    inventory: &Inventory,
    include_minimized: bool,
) -> Vec<Element> {
    complete_windows(
        application.windows(include_minimized).unwrap_or_default(),
        pid,
        inventory,
        include_minimized,
    )
}

fn complete_windows(
    published: Vec<Element>,
    pid: i32,
    inventory: &Inventory,
    include_minimized: bool,
) -> Vec<Element> {
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
    // Windows that left the WindowServer inventory have closed; drop their
    // remembered data. This must run even when the scan below is skipped.
    {
        let mut remembered = remembered_windows().lock().unwrap();
        if let Some(entries) = remembered.get_mut(&pid) {
            let alive = inventory.normal.get(&pid);
            entries.retain(|server_id, _| alive.is_some_and(|ids| ids.contains(server_id)));
        }
    }
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
            && window.switchable(include_minimized)
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
            && window.switchable(include_minimized)
        {
            // Electron can expose an AXUnknown object before the real AXWindow
            // for the same WindowServer ID. Only a validated window resolves it.
            missing.remove(&server_id);
            scan.elements.insert(server_id, element_id);
            windows.push(window);
        }
    }
    record(Level::Debug, "accessibility", "other-spaces", || {
        format!(
            "app_pid={pid} found={} unresolved={} next={}",
            scan.elements.len(),
            missing.len(),
            scan.next
        )
    });
    remote_scans().lock().unwrap().insert(pid, scan);
    windows
}

fn remember_window(pid: i32, server_id: u32, info: &WindowInfo, element: &Element) {
    // SAFETY: the raw element address stays valid because its reference count
    // is bumped here; `RememberedWindow` releases it on drop.
    let raw = element.0.as_CFTypeRef();
    unsafe { CFRetain(raw) };
    let entry = RememberedWindow {
        id: info.id,
        title: info.title.clone(),
        minimized: info.minimized,
        element: raw as usize,
    };
    remembered_windows()
        .lock()
        .unwrap()
        .entry(pid)
        .or_default()
        .insert(server_id, entry);
}

/// A previously seen window that is still alive but currently invisible to
/// accessibility queries, typically because it was minimized. The retained
/// element handle keeps answering reads and actions after minimizing.
fn remembered_element(pid: i32, id: u64) -> Option<Element> {
    let remembered = remembered_windows().lock().unwrap();
    let scan = remembered.get(&pid)?;
    let entry = scan.values().find(|entry| entry.id == id)?;
    // SAFETY: the address was retained when the window was remembered; the
    // returned wrapper takes its own reference.
    Some(Element(unsafe {
        CFType::wrap_under_get_rule(entry.element as CFTypeRef)
    }))
}

fn scan_application(
    pid: i32,
    app: &str,
    inventory: &Inventory,
    include_minimized: bool,
) -> Result<Vec<(WindowInfo, Option<u32>)>, AxError> {
    let application = Element::application(pid).ok_or(AX_NO_VALUE)?;
    let windows = all_windows(&application, pid, inventory, include_minimized);
    record(Level::Debug, "accessibility", "scan", || {
        format!("app_pid={pid} accepted={}", windows.len())
    });
    let mut results: Vec<(WindowInfo, Option<u32>)> = windows
        .into_iter()
        .map(|window| {
            let server_id = window.server_id();
            let info = WindowInfo {
                id: window.id(pid),
                pid,
                app: app.into(),
                title: window.string("AXTitle").unwrap_or_default(),
                minimized: window.boolean("AXMinimized").unwrap_or(false),
            };
            if include_minimized && let Some(server_id) = server_id {
                remember_window(pid, server_id, &info, &window);
            }
            (info, server_id)
        })
        .collect();
    if !include_minimized {
        // The user opted out of minimized-window tracking: release any data
        // remembered while the option was enabled.
        remembered_windows().lock().unwrap().remove(&pid);
        return Ok(results);
    }
    // Minimized windows become invisible to cross-process accessibility
    // queries while their WindowServer surfaces stay in the inventory. Keep
    // windows we saw earlier in the list so they remain visible and selectable.
    let mut unresolved = inventory.normal.get(&pid).cloned().unwrap_or_default();
    unresolved.retain(|server_id| !results.iter().any(|(_, found)| found == &Some(*server_id)));
    if !unresolved.is_empty() {
        // Collect the remembered data under the lock, then release it before
        // the AX reads below: each read can block for up to READ_TIMEOUT and
        // must not hold the global remembered-windows lock, which other scan
        // workers and find_window also need.
        struct Pending {
            server_id: u32,
            element_addr: usize,
            fallback_title: String,
            fallback_minimized: bool,
            id: u64,
        }
        let pending: Vec<Pending> = {
            let remembered = remembered_windows().lock().unwrap();
            let mut pending = Vec::new();
            if let Some(entries) = remembered.get(&pid) {
                for server_id in &unresolved {
                    if let Some(entry) = entries.get(server_id) {
                        pending.push(Pending {
                            server_id: *server_id,
                            element_addr: entry.element,
                            fallback_title: entry.title.clone(),
                            fallback_minimized: entry.minimized,
                            id: entry.id,
                        });
                    }
                }
            }
            pending
        };
        let mut updates = Vec::new();
        let mut new_results = Vec::new();
        for entry in pending {
            // SAFETY: the address was retained when the window was remembered;
            // the wrapper takes its own reference.
            let element =
                Element(unsafe { CFType::wrap_under_get_rule(entry.element_addr as CFTypeRef) });
            let title = element.string("AXTitle").unwrap_or(entry.fallback_title);
            let minimized = element
                .boolean("AXMinimized")
                .unwrap_or(entry.fallback_minimized);
            updates.push((entry.server_id, title.clone(), minimized));
            new_results.push((
                WindowInfo {
                    id: entry.id,
                    pid,
                    app: app.into(),
                    title,
                    minimized,
                },
                Some(entry.server_id),
            ));
        }
        // Persist the refreshed title and minimized state.
        let mut remembered = remembered_windows().lock().unwrap();
        if let Some(entries) = remembered.get_mut(&pid) {
            for (server_id, title, minimized) in updates {
                if let Some(entry) = entries.get_mut(&server_id) {
                    entry.title = title;
                    entry.minimized = minimized;
                }
            }
        }
        results.extend(new_results);
    }
    Ok(results)
}

pub fn focused_window(pid: i32, policy: FocusRead) -> Option<u64> {
    let application = Element::application(pid)?;
    let value = read_focused_window(policy, |timeout| {
        application.set_timeout(timeout);
        let start = std::time::Instant::now();
        let result = application.attribute_once("AXFocusedWindow");
        crate::macos::platform::recency_trace::record("ax-focus", || {
            format!(
                "app_pid={pid} timeout={timeout} elapsed_ms={:.3} error={:?}",
                start.elapsed().as_secs_f64() * 1000.0,
                result.as_ref().err(),
            )
        });
        result
    })
    .ok()?;
    // SAFETY: The owned attribute is checked before treating it as an AX element.
    if unsafe { CFGetTypeID(value.as_CFTypeRef()) } != unsafe { AXUIElementGetTypeID() } {
        return None;
    }
    let id = Element(value).id(pid);
    crate::macos::platform::recency_trace::record("ax-window", || format!("app_pid={pid} id={id}"));
    Some(id)
}

pub fn focused_window_server_id(pid: i32) -> Option<u32> {
    let application = Element::application(pid)?;
    let window = application.element("AXFocusedWindow").ok()?;
    window.server_id()
}

pub fn project_windows(pid: i32) -> Vec<winlane::features::projects::focus::Window> {
    if !is_trusted() {
        return Vec::new();
    }
    let Some(application) = Element::application(pid) else {
        return Vec::new();
    };
    all_windows(&application, pid, &Inventory::read(), true)
        .into_iter()
        .map(|window| winlane::features::projects::focus::Window {
            id: window.id(pid),
            title: window.string("AXTitle").unwrap_or_default(),
            document: window.string("AXDocument"),
        })
        .collect()
}

pub fn list_windows(
    apps: &[(i32, String)],
    include_minimized: bool,
) -> (Vec<WindowInfo>, HashMap<u64, u32>) {
    if !is_trusted() {
        return (Vec::new(), HashMap::new());
    }
    let inventory = Inventory::read();
    remote_scans()
        .lock()
        .unwrap()
        .retain(|pid, _| apps.iter().any(|(app_pid, _)| app_pid == pid));
    remembered_windows()
        .lock()
        .unwrap()
        .retain(|pid, _| apps.iter().any(|(app_pid, _)| app_pid == pid));
    // Each worker owns its AX handles; a slow app does not delay every other app.
    let records: Vec<_> = std::thread::scope(|scope| {
        let workers = apps
            .chunks(apps.len().div_ceil(4).max(1))
            .map(|chunk| {
                let inventory = &inventory;
                scope.spawn(move || {
                    chunk
                        .iter()
                        .flat_map(|(pid, app)| {
                            scan_application(*pid, app, inventory, include_minimized)
                                .unwrap_or_default()
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("window scan worker panicked"))
            .collect()
    });
    let server_ids = records
        .iter()
        .filter_map(|(window, id)| id.map(|id| (window.id, id)))
        .collect();
    (
        records.into_iter().map(|(window, _)| window).collect(),
        server_ids,
    )
}

fn find_window(pid: i32, id: u64) -> Result<Element, String> {
    if !is_trusted() {
        return Err(tr!(
            "尚未开启辅助功能权限。",
            "Accessibility access has not been granted."
        )
        .to_owned());
    }
    // Prefer the element retained during discovery: it is the same AX handle
    // the switcher listed, and it stays alive until its window leaves the
    // WindowServer inventory. This avoids re-reading AXWindows and the
    // inventory for every selection; the fresh scan below remains the
    // fallback for windows that were never remembered (for example when
    // minimized-window tracking is disabled).
    if let Some(window) = remembered_element(pid, id) {
        return Ok(window);
    }
    let application = Element::application(pid).ok_or(tr!(
        "应用已退出，请刷新窗口列表。",
        "The app has quit. Refresh the window list."
    ))?;
    let windows = all_windows(&application, pid, &Inventory::read(), true);
    let mut matches = windows.iter().filter(|window| window.id(pid) == id);
    if let Some(window) = matches.next() {
        if matches.any(|other| other.0 != window.0) {
            return Err(tr!(
                "无法确定目标窗口，请刷新窗口列表。",
                "Could not identify the window. Refresh the window list."
            )
            .to_owned());
        }
        return Ok(Element(window.0.clone()));
    }
    Err(tr!(
        "窗口已关闭，请刷新窗口列表。",
        "The window has closed. Refresh the window list."
    )
    .to_owned())
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
    // System Settings can reject AXRaise even for a live standard window.
    // The caller still activates the app after setting the target window's focus.
    if matches!(
        status,
        AX_SUCCESS | AX_ATTRIBUTE_UNSUPPORTED | AX_ACTION_UNSUPPORTED | AX_NOT_IMPLEMENTED
    ) {
        Ok(())
    } else {
        Err(trf!(
            "无法置前窗口（错误 {status}）。",
            "Could not raise the window (error {status})."
        ))
    }
}

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../tests/native/accessibility.rs"]
pub(crate) mod tests;
