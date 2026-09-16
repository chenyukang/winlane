use crate::main_wake::WakeHandle;
use core_foundation::base::TCFType;
use core_foundation::mach_port::{CFMachPort, CFMachPortInvalidate, CFMachPortRef};
use core_foundation::runloop::{CFRunLoop, CFRunLoopSource, kCFRunLoopCommonModes};
use objc2_foundation::MainThreadMarker;
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ptr;
use std::sync::mpsc::{self, Receiver, Sender};
use winlane::shortcuts::{Action, Binding, FLAGS_CHANGED, KEY_DOWN, KEY_UP, ShortcutRouter};

type EventRef = *mut c_void;
type TapCallback = unsafe extern "C" fn(*mut c_void, u32, EventRef, *mut c_void) -> EventRef;
const TAP_DISABLED_TIMEOUT: u32 = 0xffff_fffe;
const TAP_DISABLED_USER: u32 = 0xffff_ffff;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(
        location: u32,
        placement: u32,
        options: u32,
        mask: u64,
        callback: TapCallback,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventTapIsEnabled(tap: CFMachPortRef) -> bool;
    fn CGEventGetFlags(event: EventRef) -> u64;
    fn CGEventGetIntegerValueField(event: EventRef, field: u32) -> i64;
}

struct TapState {
    keys: RefCell<ShortcutRouter>,
    actions: Sender<Action>,
    wake: WakeHandle,
    port: Cell<CFMachPortRef>,
}

pub struct ShortcutTap {
    port: CFMachPort,
    source: CFRunLoopSource,
    _state: Box<TapState>,
    _main_thread: MainThreadMarker,
}

impl ShortcutTap {
    pub fn new(
        mtm: MainThreadMarker,
        search: Binding,
        switch: Binding,
        wake: WakeHandle,
    ) -> Result<(Self, Receiver<Action>), String> {
        if !crate::accessibility::is_trusted() {
            return Err("启用全局快捷键需要先在系统设置中允许 Winlane 控制应用。".into());
        }
        let (actions, receiver) = mpsc::channel();
        let mut state = Box::new(TapState {
            keys: RefCell::new(ShortcutRouter::new(search, switch)),
            actions,
            wake,
            port: Cell::new(ptr::null_mut()),
        });
        // SAFETY: The boxed context stays at a stable address until the tap is
        // invalidated. Session/head/default installs a filter before app routing.
        let raw = unsafe {
            CGEventTapCreate(
                1,
                0,
                0,
                (1 << KEY_DOWN) | (1 << KEY_UP) | (1 << FLAGS_CHANGED),
                callback,
                (&mut *state as *mut TapState).cast(),
            )
        };
        if raw.is_null() {
            return Err("未能启用全局快捷键。请检查 Winlane 的系统授权，再保存设置重试。".into());
        }
        // SAFETY: Create returned a non-null, retained Mach port owned here.
        let port = unsafe { CFMachPort::wrap_under_create_rule(raw) };
        state.port.set(raw);
        let source = match port.create_runloop_source(0) {
            Ok(source) => source,
            Err(()) => {
                // SAFETY: Remove the callback before its boxed context is freed.
                unsafe { CFMachPortInvalidate(raw) };
                return Err("无法启动快捷键监听，请重新打开 Winlane。".into());
            }
        };
        // SAFETY: The system mode constant is valid for this run loop's lifetime.
        CFRunLoop::get_main().add_source(&source, unsafe { kCFRunLoopCommonModes });
        let tap = Self {
            port,
            source,
            _state: state,
            _main_thread: mtm,
        };
        if !tap.is_enabled() {
            return Err("快捷键监听未启用，请检查系统授权。".into());
        }
        Ok((tap, receiver))
    }

    pub fn open_search(&self) -> Action {
        self._state.keys.borrow_mut().open_search()
    }

    pub fn enter_switch(&self, flags: u64) -> Action {
        self._state.keys.borrow_mut().enter_switch(flags)
    }

    pub fn finish(&self, session: u64) {
        self._state.keys.borrow_mut().finish(session);
    }

    pub fn resume_search(&self, session: u64) {
        self._state.keys.borrow_mut().resume_search(session);
    }

    pub fn cancel(&self) {
        self._state.keys.borrow_mut().cancel();
    }

    pub fn is_enabled(&self) -> bool {
        // SAFETY: This instance retains the port and is only used on the main thread.
        unsafe { CGEventTapIsEnabled(self.port.as_concrete_TypeRef()) }
    }
}

impl Drop for ShortcutTap {
    fn drop(&mut self) {
        // SAFETY: Destruction runs on the same thread as callbacks. Invalidate
        // before releasing the run-loop source and callback context.
        unsafe { CFMachPortInvalidate(self.port.as_concrete_TypeRef()) };
        CFRunLoop::get_main().remove_source(&self.source, unsafe { kCFRunLoopCommonModes });
    }
}

unsafe extern "C" fn callback(
    _: *mut c_void,
    event_type: u32,
    event: EventRef,
    user_info: *mut c_void,
) -> EventRef {
    if user_info.is_null() {
        return event;
    }
    // SAFETY: ShortcutTap owns this context until after invalidating the port.
    // The callback runs on the main run loop and never calls AppKit or waits.
    let state = unsafe { &*user_info.cast::<TapState>() };
    if event_type == TAP_DISABLED_TIMEOUT || event_type == TAP_DISABLED_USER {
        if let Ok(mut keys) = state.keys.try_borrow_mut() {
            let _ = state.actions.send(keys.cancel());
            state.wake.signal();
        }
        if event_type == TAP_DISABLED_TIMEOUT {
            // SAFETY: The tap retains its own port while a callback is in flight.
            unsafe { CGEventTapEnable(state.port.get(), true) };
        }
        return event;
    }
    if event.is_null() || !matches!(event_type, KEY_DOWN | KEY_UP | FLAGS_CHANGED) {
        return event;
    }
    // SAFETY: CoreGraphics supplies a live keyboard event for this callback.
    let (key, flags, repeat) = unsafe {
        (
            CGEventGetIntegerValueField(event, 9),
            CGEventGetFlags(event),
            CGEventGetIntegerValueField(event, 8) != 0,
        )
    };
    let Ok(mut keys) = state.keys.try_borrow_mut() else {
        return event;
    };
    let (consume, action) = keys.handle(event_type, key, flags, repeat);
    if let Some(action) = action {
        if state.actions.send(action).is_err() {
            return event;
        }
        state.wake.signal();
    }
    if consume { ptr::null_mut() } else { event }
}
