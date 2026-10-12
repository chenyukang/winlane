use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
use objc2_core_graphics::{CGEvent, CGEventTapLocation};
use objc2_foundation::NSPoint;

// Media key codes from IOKit/hidsystem/ev_keymap.h. macOS expects them in the
// high half of a system-defined event's data1 field.
pub(crate) const SOUND_UP: i32 = 0;
pub(crate) const SOUND_DOWN: i32 = 1;
pub(crate) const MUTE: i32 = 7;
// The keyboard's play/pause key. It toggles whichever player macOS considers
// current, the same as pressing the key by hand.
pub(crate) const PLAY_PAUSE: i32 = 16;

// NX_SUBTYPE_AUX_CONTROL_BUTTONS, the subtype macOS uses for media keys.
const AUX_CONTROL_BUTTONS: i16 = 8;
// Low bits of data1 that tell the system whether the media key went down or up.
const KEY_DOWN: u64 = 0x0A00;
const KEY_UP: u64 = 0x0B00;

/// Callable that posts a media key. Tests replace it to observe dispatch.
pub(crate) type Post = fn(i32);

/// Posts one press-and-release of a media key. macOS performs the volume change
/// itself and shows the same HUD it shows for the keyboard's own volume keys.
pub(crate) fn post(key: i32) {
    for state in [KEY_DOWN, KEY_UP] {
        post_state(key, state);
    }
}

fn post_state(key: i32, state: u64) {
    let data1 = (i64::from(key) << 16) | state as i64;
    let event = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
        NSEventType::SystemDefined,
        NSPoint { x: 0.0, y: 0.0 },
        NSEventModifierFlags(state as usize),
        0.0,
        0,
        None,
        AUX_CONTROL_BUTTONS,
        data1 as isize,
        -1,
    );
    // The window server copies the event, so it does not outlive this call.
    if let Some(event) = event.and_then(|event| event.CGEvent()) {
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
    }
}
