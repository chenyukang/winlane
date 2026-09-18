use block2::RcBlock;
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use objc2::rc::Retained;
use objc2_app_kit::{
    NSEvent, NSEventModifierFlags, NSPasteboard, NSPasteboardTypeString, NSRunningApplication,
    NSWorkspace,
};
use objc2_foundation::{MainThreadMarker, NSString, NSTimer};
use std::cell::Cell;
use std::time::Instant;
use winlane::tr;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreateKeyboardEvent(source: CFTypeRef, key: u16, down: bool) -> CFTypeRef;
    fn CGEventSetFlags(event: CFTypeRef, flags: u64);
    fn CGEventPostToPid(pid: i32, event: CFTypeRef);
}

pub fn start(
    target: Retained<NSRunningApplication>,
    text: String,
    mtm: MainThreadMarker,
    report: impl Fn(String) + 'static,
) -> Result<Retained<NSTimer>, String> {
    if target.isTerminated() || target.processIdentifier() == std::process::id() as i32 {
        return Err(tr!(
            "目标应用已退出。请从要粘贴的应用重新打开搜索。",
            "The target app has quit. Open search from the app you want to paste into."
        )
        .into());
    }
    if !crate::accessibility::is_trusted() {
        return Err(tr!(
            "粘贴需要辅助功能权限。",
            "Pasting requires Accessibility access."
        )
        .into());
    }
    // Prepare both events before changing the clipboard or focus.
    let event = |down| {
        let ptr = unsafe { CGEventCreateKeyboardEvent(std::ptr::null(), 9, down) };
        if ptr.is_null() {
            return Err(
                tr!("无法创建粘贴事件。", "Could not prepare the paste event.").to_string(),
            );
        }
        let event = unsafe { CFType::wrap_under_create_rule(ptr) };
        unsafe {
            CGEventSetFlags(event.as_CFTypeRef(), 1 << 20);
        }
        Ok(event)
    };
    let down = event(true)?;
    let up = event(false)?;
    let pid = target.processIdentifier();
    let started = Instant::now();
    let ready_since = Cell::new(None::<Instant>);
    #[allow(deprecated)]
    if !target.activateWithOptions(
        objc2_app_kit::NSApplicationActivationOptions::ActivateIgnoringOtherApps,
    ) {
        return Err(tr!("无法聚焦目标应用。", "Could not focus the target app.").into());
    }
    let callback = RcBlock::new(move |timer: std::ptr::NonNull<NSTimer>| {
        let timer = unsafe { timer.as_ref() };
        let front = NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|app| app.processIdentifier());
        if target.isTerminated()
            || started.elapsed().as_secs_f64() > 2.0
            || front.is_some_and(|front| front != pid && front != std::process::id() as i32)
        {
            timer.invalidate();
            report(
                tr!(
                    "未粘贴：目标应用没有聚焦，或焦点已改变。",
                    "Not pasted: the target app did not gain focus, or focus changed."
                )
                .into(),
            );
            return;
        }
        if front != Some(pid) {
            ready_since.set(None);
            return;
        }
        let ready = ready_since.get().unwrap_or_else(|| {
            let now = Instant::now();
            ready_since.set(Some(now));
            now
        });
        let modifiers = NSEvent::modifierFlags_class();
        if ready.elapsed().as_millis() < 100
            || modifiers.intersects(
                NSEventModifierFlags::Command
                    | NSEventModifierFlags::Control
                    | NSEventModifierFlags::Option
                    | NSEventModifierFlags::Shift,
            )
        {
            return;
        }
        timer.invalidate();
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        if !pasteboard.setString_forType(&NSString::from_str(&text), unsafe {
            NSPasteboardTypeString
        }) {
            report(tr!("无法写入剪贴板。", "Could not write to the clipboard.").into());
            return;
        }
        // Target the captured process, never whichever app happens to receive global keys.
        unsafe {
            CGEventPostToPid(pid, down.as_CFTypeRef());
            CGEventPostToPid(pid, up.as_CFTypeRef());
        }
        let _ = mtm;
    });
    Ok(unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(0.025, true, &callback) })
}
