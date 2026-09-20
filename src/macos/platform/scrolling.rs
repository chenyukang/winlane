use core_foundation::base::{CFRelease, TCFType};
use core_foundation::mach_port::{CFMachPort, CFMachPortInvalidate, CFMachPortRef};
use core_foundation::runloop::{CFRunLoop, CFRunLoopSource, kCFRunLoopCommonModes};
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSEvent, NSTouchPhase};
use objc2_core_graphics::CGEvent;
use objc2_foundation::MainThreadMarker;
use std::cell::{Cell, RefCell};
use std::ffi::{c_char, c_void};
use std::ptr;
use std::time::Instant;
use winlane::features::scrolling::{Classifier, Delta, Settings};
use winlane::tr;

type Event = *mut c_void;
type Callback = unsafe extern "C" fn(*mut c_void, u32, Event, *mut c_void) -> Event;
const SCROLL: u32 = 22;
const GESTURE: u32 = 29;
const TIMEOUT: u32 = 0xffff_fffe;
const USER_DISABLED: u32 = 0xffff_ffff;
const LINES: [u32; 2] = [11, 12];
const POINTS: [u32; 2] = [96, 97];
const FIXED: [u32; 2] = [93, 94];

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(
        location: u32,
        placement: u32,
        options: u32,
        mask: u64,
        callback: Callback,
        context: *mut c_void,
    ) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enabled: bool);
    fn CGEventTapIsEnabled(tap: CFMachPortRef) -> bool;
    fn CGEventGetIntegerValueField(event: Event, field: u32) -> i64;
    fn CGEventSetIntegerValueField(event: Event, field: u32, value: i64);
    fn CGEventGetDoubleValueField(event: Event, field: u32) -> f64;
    fn CGEventSetDoubleValueField(event: Event, field: u32, value: f64);
}

struct State {
    settings: RefCell<Settings>,
    classifier: RefCell<Classifier>,
    clock: Instant,
    gesture: Cell<CFMachPortRef>,
    scroll: Cell<CFMachPortRef>,
    hid: Option<HidApi>,
}

struct Tap {
    port: CFMachPort,
    source: CFRunLoopSource,
}
impl Tap {
    fn new(state: &State, gesture: bool) -> Result<Self, String> {
        // SAFETY: The owning ScrollTap keeps State boxed until both ports are invalidated.
        let raw = unsafe {
            CGEventTapCreate(
                1,
                1,
                u32::from(gesture),
                1 << if gesture { GESTURE } else { SCROLL },
                if gesture {
                    gesture_callback
                } else {
                    scroll_callback
                },
                (state as *const State).cast_mut().cast(),
            )
        };
        if raw.is_null() {
            return Err(permission_error());
        }
        let port = unsafe { CFMachPort::wrap_under_create_rule(raw) };
        let source = match port.create_runloop_source(0) {
            Ok(source) => source,
            Err(()) => {
                unsafe { CFMachPortInvalidate(raw) };
                return Err(permission_error());
            }
        };
        // Keep callbacks disabled until the complete pair of taps is ready.
        unsafe { CGEventTapEnable(raw, false) };
        CFRunLoop::get_main().add_source(&source, unsafe { kCFRunLoopCommonModes });
        Ok(Self { port, source })
    }
    fn raw(&self) -> CFMachPortRef {
        self.port.as_concrete_TypeRef()
    }
}
impl Drop for Tap {
    fn drop(&mut self) {
        unsafe { CFMachPortInvalidate(self.raw()) };
        CFRunLoop::get_main().remove_source(&self.source, unsafe { kCFRunLoopCommonModes });
    }
}

pub struct ScrollTap {
    // Ports must be dropped before their callback context.
    gesture: Tap,
    scroll: Tap,
    state: Box<State>,
    _main_thread: MainThreadMarker,
}
impl ScrollTap {
    pub fn prepare(settings: &Settings, mtm: MainThreadMarker) -> Result<Self, String> {
        settings.validate()?;
        if !super::accessibility::is_trusted() {
            return Err(permission_error());
        }
        let state = Box::new(State {
            settings: RefCell::new(settings.clone()),
            classifier: RefCell::new(Classifier::default()),
            clock: Instant::now(),
            gesture: Cell::new(ptr::null_mut()),
            scroll: Cell::new(ptr::null_mut()),
            hid: HidApi::load(),
        });
        let gesture = Tap::new(&state, true)?;
        state.gesture.set(gesture.raw());
        let scroll = Tap::new(&state, false)?;
        state.scroll.set(scroll.raw());
        Ok(Self {
            gesture,
            scroll,
            state,
            _main_thread: mtm,
        })
    }
    pub fn start(&self) -> Result<(), String> {
        self.state.classifier.replace(Classifier::default());
        unsafe {
            CGEventTapEnable(self.gesture.raw(), true);
            CGEventTapEnable(self.scroll.raw(), true);
        }
        if self.is_enabled() {
            Ok(())
        } else {
            Err(permission_error())
        }
    }
    pub fn is_enabled(&self) -> bool {
        unsafe { CGEventTapIsEnabled(self.gesture.raw()) && CGEventTapIsEnabled(self.scroll.raw()) }
    }
    pub fn configure(&self, settings: &Settings) {
        self.state.settings.replace(settings.clone());
    }
}
fn permission_error() -> String {
    tr!("无法启用滚轮设置。请在系统设置的“辅助功能”和“输入监控”中允许 Winlane，再点击重试。", "Could not enable scrolling. Allow Winlane in System Settings > Accessibility and Input Monitoring, then retry.").into()
}

unsafe extern "C" fn gesture_callback(
    _: *mut c_void,
    kind: u32,
    event: Event,
    context: *mut c_void,
) -> Event {
    unsafe { callback(true, kind, event, context) }
}
unsafe extern "C" fn scroll_callback(
    _: *mut c_void,
    kind: u32,
    event: Event,
    context: *mut c_void,
) -> Event {
    unsafe { callback(false, kind, event, context) }
}
unsafe fn callback(gesture: bool, kind: u32, event: Event, context: *mut c_void) -> Event {
    if context.is_null() {
        return event;
    }
    // SAFETY: Main-thread callbacks cannot outlive the ports or their boxed state.
    let state = unsafe { &*context.cast::<State>() };
    if matches!(kind, TIMEOUT | USER_DISABLED) {
        if let Ok(mut classifier) = state.classifier.try_borrow_mut() {
            *classifier = Classifier::default();
        }
        if kind == TIMEOUT {
            unsafe {
                CGEventTapEnable(
                    if gesture {
                        state.gesture.get()
                    } else {
                        state.scroll.get()
                    },
                    true,
                )
            };
        }
        return event;
    }
    if event.is_null() {
        return event;
    }
    let Ok(mut classifier) = state.classifier.try_borrow_mut() else {
        return event;
    };
    let now = state.clock.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    if gesture && kind == GESTURE {
        autoreleasepool(|_| {
            // SAFETY: AppKit converts a live CGEvent; touches are only inspected on gesture events.
            let native = NSEvent::eventWithCGEvent(unsafe { &*event.cast::<CGEvent>() });
            if let Some(native) = native {
                classifier.touch(
                    native
                        .touchesMatchingPhase_inView(NSTouchPhase::Touching, None)
                        .count(),
                    now,
                );
            }
        });
    } else if !gesture && kind == SCROLL {
        // Ignore application-generated scrolling (e.g. remote desktop and other modifiers).
        if unsafe { CGEventGetIntegerValueField(event, 41) } != 0 {
            return event;
        }
        // If touch monitoring is unavailable, do not guess that a trackpad is a mouse.
        if !unsafe { CGEventTapIsEnabled(state.gesture.get()) } {
            return event;
        }
        let continuous = unsafe { CGEventGetIntegerValueField(event, 88) } != 0;
        let momentum = unsafe { CGEventGetIntegerValueField(event, 123) } != 0;
        let device = classifier.classify(continuous, momentum, now);
        let Ok(settings) = state.settings.try_borrow() else {
            return event;
        };
        let lines = unsafe { CGEventGetIntegerValueField(event, LINES[0]) };
        let multipliers = settings.multipliers(device, continuous, lines);
        unsafe { transform(event, multipliers, state.hid.as_ref()) };
    }
    event
}

unsafe fn transform(event: Event, multipliers: [i64; 2], hid: Option<&HidApi>) {
    if multipliers == [1, 1] {
        return;
    }
    // Read all representations before setting line deltas, which also rewrites the others.
    let deltas = [0, 1].map(|axis| unsafe {
        Delta {
            lines: CGEventGetIntegerValueField(event, LINES[axis]),
            points: CGEventGetIntegerValueField(event, POINTS[axis]),
            fixed: CGEventGetDoubleValueField(event, FIXED[axis]),
        }
        .scaled(multipliers[axis])
    });
    unsafe {
        if let Some(hid) = hid {
            hid.scale(event, multipliers);
        }
        for axis in 0..2 {
            if multipliers[axis] != 1 {
                CGEventSetIntegerValueField(event, LINES[axis], deltas[axis].lines);
            }
        }
        // Setting either line axis may change the other axis's derived representations.
        for axis in 0..2 {
            CGEventSetDoubleValueField(event, FIXED[axis], deltas[axis].fixed);
            CGEventSetIntegerValueField(event, POINTS[axis], deltas[axis].points);
        }
    }
}

// WebKit can consume the attached HID deltas instead of CGEvent deltas. These optional
// SPI symbols are resolved once, outside callbacks; older systems retain the CG fallback.
struct HidApi {
    copy: unsafe extern "C" fn(Event) -> Event,
    get: unsafe extern "C" fn(Event, u32) -> f64,
    set: unsafe extern "C" fn(Event, u32, f64),
}
impl HidApi {
    fn load() -> Option<Self> {
        unsafe extern "C" {
            fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
        }
        unsafe {
            let copy = dlsym((-2isize) as *mut c_void, c"CGEventCopyIOHIDEvent".as_ptr());
            let get = dlsym(
                (-2isize) as *mut c_void,
                c"IOHIDEventGetFloatValue".as_ptr(),
            );
            let set = dlsym(
                (-2isize) as *mut c_void,
                c"IOHIDEventSetFloatValue".as_ptr(),
            );
            if copy.is_null() || get.is_null() || set.is_null() {
                return None;
            }
            Some(Self {
                copy: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(Event) -> Event>(
                    copy,
                ),
                get: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(Event, u32) -> f64>(
                    get,
                ),
                set: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(Event, u32, f64)>(set),
            })
        }
    }
    unsafe fn scale(&self, event: Event, multipliers: [i64; 2]) {
        unsafe {
            let hid = (self.copy)(event);
            if hid.is_null() {
                return;
            }
            for (field, factor) in [(0x60001, multipliers[0]), (0x60000, multipliers[1])] {
                if factor != 1 {
                    (self.set)(hid, field, (self.get)(hid, field) * factor as f64);
                }
            }
            CFRelease(hid.cast_const());
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/native/scrolling.rs"]
pub(crate) mod tests;
