use crate::core::search::WindowInfo;

pub const AX_CANNOT_COMPLETE: i32 = -25204;
pub const AX_NO_VALUE: i32 = -25212;
pub const READ_TIMEOUT: f32 = 0.12;
const RETRY_TIMEOUT: f32 = 0.8;

#[derive(Clone, Copy)]
pub enum FocusRead {
    Immediate,
    Background,
}

pub fn read_focused_window<T>(
    policy: FocusRead,
    mut read: impl FnMut(f32) -> Result<T, i32>,
) -> Result<T, i32> {
    match policy {
        FocusRead::Immediate => read(0.02),
        FocusRead::Background => match read(READ_TIMEOUT) {
            // Activation can arrive before the app exposes its focused window.
            Err(AX_CANNOT_COMPLETE | AX_NO_VALUE) => read(RETRY_TIMEOUT),
            result => result,
        },
    }
}

pub fn read_with_retry<T>(mut read: impl FnMut(f32) -> Result<T, i32>) -> Result<T, i32> {
    match read(READ_TIMEOUT) {
        Err(AX_CANNOT_COMPLETE) => read(RETRY_TIMEOUT),
        result => result,
    }
}

pub fn merge_window_sources<T: PartialEq>(
    primary: Result<Vec<T>, i32>,
    secondary: Result<Vec<T>, i32>,
) -> Result<Vec<T>, i32> {
    if let (Err(error), Err(_)) = (&primary, &secondary) {
        return Err(*error);
    }
    let mut merged = Vec::new();
    for window in primary
        .unwrap_or_default()
        .into_iter()
        .chain(secondary.unwrap_or_default())
    {
        if !merged.contains(&window) {
            merged.push(window);
        }
    }
    Ok(merged)
}

pub fn finish_application_scan(windows: Result<Vec<WindowInfo>, i32>) -> Vec<WindowInfo> {
    windows.unwrap_or_default()
}

pub fn read_published_windows<T: PartialEq>(
    mut read: impl FnMut(&str) -> Result<Vec<T>, i32>,
) -> Result<Vec<T>, i32> {
    let mut windows = merge_window_sources(read("AXWindows"), read("AXChildren"));
    for attribute in ["AXFocusedWindow", "AXMainWindow"] {
        windows = merge_window_sources(windows, read(attribute));
    }
    windows
}

pub fn normal_window_surface(layer: i64, alpha: f64, width: f64, height: f64) -> bool {
    layer == 0
        && alpha.is_finite()
        && alpha > 0.0
        && width.is_finite()
        && width > 1.0
        && height.is_finite()
        && height > 1.0
}

pub fn remote_window_token(pid: i32, element_id: u64) -> [u8; 20] {
    let mut bytes = [0; 20];
    bytes[..4].copy_from_slice(&pid.to_ne_bytes());
    bytes[8..12].copy_from_slice(&0x636f636fu32.to_ne_bytes());
    bytes[12..].copy_from_slice(&element_id.to_ne_bytes());
    bytes
}

pub fn switchable_window(role: Option<&str>, subrole: Option<&str>) -> bool {
    role == Some("AXWindow") && subrole == Some("AXStandardWindow")
}

/// A window worth listing in the switcher, including minimized windows.
///
/// macOS hides minimized windows from cross-process window-list queries and
/// reports AppKit ones as dialogs (`AXDialog`) when they are reachable at
/// all. A minimized `AXDialog` is still a switchable window: selecting it
/// restores and focuses it. Non-minimized dialogs stay excluded.
pub fn switchable_window_role(
    role: Option<&str>,
    subrole: Option<&str>,
    minimized: Option<bool>,
) -> bool {
    switchable_window(role, subrole)
        || (role == Some("AXWindow") && subrole == Some("AXDialog") && minimized == Some(true))
}
