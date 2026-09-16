use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::runloop::{
    CFRunLoop, CFRunLoopSource, CFRunLoopSourceRef, kCFRunLoopCommonModes,
};
use core_foundation::string::{CFString, CFStringRef};
use objc2_foundation::MainThreadMarker;
use std::ffi::c_void;
use std::ptr;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXObserverCreate(
        pid: i32,
        callback: extern "C" fn(CFTypeRef, CFTypeRef, CFStringRef, *mut c_void),
        observer: *mut CFTypeRef,
    ) -> i32;
    fn AXObserverGetRunLoopSource(observer: CFTypeRef) -> CFRunLoopSourceRef;
    fn AXObserverAddNotification(
        observer: CFTypeRef,
        element: CFTypeRef,
        name: CFStringRef,
        context: *mut c_void,
    ) -> i32;
    fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
}

struct Context {
    pid: i32,
    callback: Box<dyn Fn(u64)>,
}

pub struct FocusObserver {
    pub pid: i32,
    source: CFRunLoopSource,
    _observer: CFType,
    _context: Box<Context>,
    _main_thread: MainThreadMarker,
}

impl FocusObserver {
    pub fn new(pid: i32, mtm: MainThreadMarker, callback: impl Fn(u64) + 'static) -> Option<Self> {
        if pid <= 0 || !crate::accessibility::is_trusted() {
            return None;
        }
        let mut raw = ptr::null();
        // SAFETY: The callback has the AXObserver ABI; the out-pointer is valid.
        if unsafe { AXObserverCreate(pid, changed, &mut raw) } != 0 || raw.is_null() {
            return None;
        }
        let observer = unsafe { CFType::wrap_under_create_rule(raw) };
        let application = unsafe { AXUIElementCreateApplication(pid) };
        if application.is_null() {
            return None;
        }
        let application = unsafe { CFType::wrap_under_create_rule(application) };
        let mut context = Box::new(Context {
            pid,
            callback: Box::new(callback),
        });
        let notification = CFString::new("AXFocusedWindowChanged");
        // SAFETY: Registration retains the elements; the boxed context stays alive
        // until the source is removed on this same main thread.
        if unsafe {
            AXObserverAddNotification(
                observer.as_CFTypeRef(),
                application.as_CFTypeRef(),
                notification.as_concrete_TypeRef(),
                (&mut *context as *mut Context).cast(),
            )
        } != 0
        {
            return None;
        }
        let source = unsafe {
            CFRunLoopSource::wrap_under_get_rule(AXObserverGetRunLoopSource(
                observer.as_CFTypeRef(),
            ))
        };
        CFRunLoop::get_main().add_source(&source, unsafe { kCFRunLoopCommonModes });
        Some(Self {
            pid,
            source,
            _observer: observer,
            _context: context,
            _main_thread: mtm,
        })
    }
}

impl Drop for FocusObserver {
    fn drop(&mut self) {
        CFRunLoop::get_main().remove_source(&self.source, unsafe { kCFRunLoopCommonModes });
    }
}

extern "C" fn changed(_: CFTypeRef, _: CFTypeRef, _: CFStringRef, context: *mut c_void) {
    // SAFETY: Only the main run loop dispatches this source; the owner removes
    // it before dropping the boxed context or observer.
    let context = unsafe { &*context.cast::<Context>() };
    objc2::rc::autoreleasepool(|_| {
        if let Some(id) = crate::accessibility::focused_window(context.pid) {
            (context.callback)(id);
        }
    });
}
