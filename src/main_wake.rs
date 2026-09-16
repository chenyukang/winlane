use core_foundation::base::TCFType;
use core_foundation::runloop::{
    CFRunLoop, CFRunLoopSource, CFRunLoopSourceContext, CFRunLoopSourceCreate,
    CFRunLoopSourceInvalidate, CFRunLoopSourceSignal, CFRunLoopWakeUp, kCFRunLoopCommonModes,
};
use objc2_foundation::MainThreadMarker;
use std::ffi::c_void;
use std::ptr;

struct Context(Box<dyn Fn()>);

pub struct MainWake {
    handle: WakeHandle,
    _context: Box<Context>,
    _main_thread: MainThreadMarker,
}

#[derive(Clone)]
pub struct WakeHandle {
    source: CFRunLoopSource,
    run_loop: CFRunLoop,
}

// SAFETY: Handles only retain/release and signal Core Foundation objects, which
// are thread-safe. The callback and its context are accessed only on the main
// run loop; MainWake invalidates the source before releasing that context.
unsafe impl Send for WakeHandle {}
unsafe impl Sync for WakeHandle {}

impl MainWake {
    pub fn new(mtm: MainThreadMarker, callback: impl Fn() + 'static) -> Self {
        let mut context = Box::new(Context(Box::new(callback)));
        let mut descriptor = CFRunLoopSourceContext {
            version: 0,
            info: (&mut *context as *mut Context).cast(),
            retain: None,
            release: None,
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform,
        };
        // SAFETY: The stable boxed context outlives this registered source.
        let raw = unsafe { CFRunLoopSourceCreate(ptr::null(), 0, &mut descriptor) };
        assert!(!raw.is_null(), "could not create the main run-loop source");
        let source = unsafe { CFRunLoopSource::wrap_under_create_rule(raw) };
        let run_loop = CFRunLoop::get_main();
        run_loop.add_source(&source, unsafe { kCFRunLoopCommonModes });
        Self {
            handle: WakeHandle { source, run_loop },
            _context: context,
            _main_thread: mtm,
        }
    }

    pub fn handle(&self) -> WakeHandle {
        self.handle.clone()
    }

    pub fn signal(&self) {
        self.handle.signal();
    }
}

impl WakeHandle {
    pub fn signal(&self) {
        // SAFETY: Handles retain both objects, including after source invalidation.
        // Signaling only queues work; it never invokes the callback inline.
        unsafe {
            CFRunLoopSourceSignal(self.source.as_concrete_TypeRef());
            CFRunLoopWakeUp(self.run_loop.as_concrete_TypeRef());
        }
    }
}

impl Drop for MainWake {
    fn drop(&mut self) {
        // SAFETY: Destruction and callbacks share the main thread. Invalidation
        // prevents any later handle signal from reaching the boxed context.
        unsafe { CFRunLoopSourceInvalidate(self.handle.source.as_concrete_TypeRef()) };
        self.handle
            .run_loop
            .remove_source(&self.handle.source, unsafe { kCFRunLoopCommonModes });
    }
}

extern "C" fn perform(info: *const c_void) {
    // SAFETY: The source is main-thread-only and MainWake owns this context.
    let context = unsafe { &*info.cast::<Context>() };
    objc2::rc::autoreleasepool(|_| (context.0)());
}
