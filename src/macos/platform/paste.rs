use block2::RcBlock;
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use objc2::rc::Retained;
use objc2_app_kit::{
    NSEvent, NSEventModifierFlags, NSPasteboard, NSRunningApplication, NSWorkspace,
};
use objc2_foundation::{MainThreadMarker, NSTimer};
use std::cell::{Cell, RefCell};
use std::time::Instant;
use winlane::tr;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreateKeyboardEvent(source: CFTypeRef, key: u16, down: bool) -> CFTypeRef;
    fn CGEventSetFlags(event: CFTypeRef, flags: u64);
    fn CGEventPostToPid(pid: i32, event: CFTypeRef);
}

/// Where the caret belongs after the paste: how many characters at the end of
/// the text come after it, from a snippet's `{cursor}`.
pub const CARET_AT_END: usize = 0;
/// A paste needs a moment before the caret can be moved inside it.
const CARET_DELAY_MS: u128 = 60;
/// Text this long after the cursor is left at the end instead; sending
/// hundreds of key events would be worse than ignoring the marker.
const CARET_STEP_LIMIT: usize = 400;

pub fn start(
    target: Retained<NSRunningApplication>,
    text: String,
    trailing: usize,
    mtm: MainThreadMarker,
    report: impl Fn(String) + 'static,
) -> Result<Retained<NSTimer>, String> {
    start_content(
        target,
        crate::macos::platform::clipboard::PasteContent::Text(text),
        trailing,
        mtm,
        report,
    )
}

pub fn start_content(
    target: Retained<NSRunningApplication>,
    content: crate::macos::platform::clipboard::PasteContent,
    trailing: usize,
    mtm: MainThreadMarker,
    report: impl Fn(String) + 'static,
) -> Result<Retained<NSTimer>, String> {
    let content = RefCell::new(Some(content));
    if target.isTerminated() || target.processIdentifier() == std::process::id() as i32 {
        return Err(tr!(
            "目标应用已退出。请从要粘贴的应用重新打开搜索。",
            "The target app has quit. Open search from the app you want to paste into."
        )
        .into());
    }
    if !crate::macos::platform::accessibility::is_trusted() {
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
    // The caret moves with the same mechanism the paste uses, prepared now so
    // nothing is allocated while the target app is being driven.
    let arrow = |down| {
        let ptr = unsafe { CGEventCreateKeyboardEvent(std::ptr::null(), 123, down) };
        if ptr.is_null() {
            return Err(tr!(
                "无法创建移动光标的事件。",
                "Could not prepare the caret event."
            )
            .to_string());
        }
        Ok(unsafe { CFType::wrap_under_create_rule(ptr) })
    };
    let arrow_down = arrow(true)?;
    let arrow_up = arrow(false)?;
    let trailing = trailing.min(CARET_STEP_LIMIT);
    let pid = target.processIdentifier();
    let started = Instant::now();
    let ready_since = Cell::new(None::<Instant>);
    #[allow(deprecated)]
    if !target.activateWithOptions(
        objc2_app_kit::NSApplicationActivationOptions::ActivateIgnoringOtherApps,
    ) {
        return Err(tr!("无法聚焦目标应用。", "Could not focus the target app.").into());
    }
    let pasted_at = Cell::new(None::<Instant>);
    let callback = RcBlock::new(move |timer: std::ptr::NonNull<NSTimer>| {
        let timer = unsafe { timer.as_ref() };
        // The paste has been sent: the caret can only move once the app has
        // applied it, so the keys follow after a short pause.
        if let Some(at) = pasted_at.get() {
            if at.elapsed().as_millis() >= CARET_DELAY_MS {
                for _ in 0..trailing {
                    unsafe {
                        CGEventPostToPid(pid, arrow_down.as_CFTypeRef());
                        CGEventPostToPid(pid, arrow_up.as_CFTypeRef());
                    }
                }
                timer.invalidate();
            }
            return;
        }
        let front = NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|app| app.processIdentifier());
        if target.isTerminated()
            || started.elapsed().as_secs_f64() > 2.0
            || front.is_some_and(|front| front != pid && front != std::process::id() as i32)
        {
            timer.invalidate();
            content.borrow_mut().take();
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
        let pasteboard = NSPasteboard::generalPasteboard();
        let Some(value) = content.borrow_mut().take() else {
            timer.invalidate();
            return;
        };
        if let Err(error) = value.write(&pasteboard) {
            timer.invalidate();
            report(error);
            return;
        }
        // Target the captured process, never whichever app happens to receive global keys.
        unsafe {
            CGEventPostToPid(pid, down.as_CFTypeRef());
            CGEventPostToPid(pid, up.as_CFTypeRef());
        }
        if trailing == 0 {
            timer.invalidate();
        } else {
            pasted_at.set(Some(Instant::now()));
        }
        let _ = mtm;
    });
    Ok(unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(0.025, true, &callback) })
}
