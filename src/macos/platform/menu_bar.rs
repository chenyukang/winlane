use core_foundation::base::{CFType, CFTypeRef, TCFType};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect};
use std::ptr;
use winlane::tr;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGDisplayIsActive(display: u32) -> u32;
    fn CGDisplayBounds(display: u32) -> NSRect;
    fn CGEventCreateMouseEvent(
        source: CFTypeRef,
        event_type: u32,
        position: NSPoint,
        button: u32,
    ) -> CFTypeRef;
    fn CGEventPost(location: u32, event: CFTypeRef);
}

pub struct Reveal {
    event: CFType,
    _main_thread: MainThreadMarker,
}

impl Reveal {
    pub fn prepare(display: u32, mtm: MainThreadMarker) -> Result<Self, String> {
        if !crate::macos::platform::accessibility::is_trusted() {
            return Err(tr!(
                "请先在系统设置中允许 Winlane 的辅助功能权限。",
                "Allow Winlane Accessibility access in System Settings first."
            )
            .into());
        }
        // SAFETY: These functions accept display IDs and return values without transferring ownership.
        let bounds = unsafe {
            if CGDisplayIsActive(display) == 0 {
                return Err(tr!(
                    "屏幕已断开，请重新打开搜索。",
                    "Display disconnected. Reopen search and try again."
                )
                .into());
            }
            CGDisplayBounds(display)
        };
        let position = reveal_position(bounds);
        // SAFETY: A mouse-moved event (5) never clicks. Quartz uses global top-left display coordinates.
        let raw = unsafe { CGEventCreateMouseEvent(ptr::null(), 5, position, 0) };
        if raw.is_null() {
            return Err(tr!(
                "无法显示菜单栏，请重试。",
                "Could not show the menu bar. Try again."
            )
            .into());
        }
        Ok(Self {
            // SAFETY: The non-null Create result transfers one retain to this owner.
            event: unsafe { CFType::wrap_under_create_rule(raw) },
            _main_thread: mtm,
        })
    }

    pub fn show(self) {
        // SAFETY: The retained mouse-moved event is posted at the HID event tap (0).
        // Leave the pointer at the edge; restoring it immediately would hide the bar again.
        unsafe { CGEventPost(0, self.event.as_CFTypeRef()) };
    }
}

fn reveal_position(bounds: NSRect) -> NSPoint {
    // Stay clear of hot corners and the camera notch, within this display even in stacked layouts.
    NSPoint::new(
        bounds.origin.x + bounds.size.width * 0.75,
        bounds.origin.y + 1.0,
    )
}

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../tests/native/menu_bar.rs"]
pub(crate) mod tests;
