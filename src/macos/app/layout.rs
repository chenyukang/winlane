use super::*;

pub(super) fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}
pub(super) fn row_height(density: DisplayDensity) -> f64 {
    match density {
        DisplayDensity::Compact => 28.0,
        DisplayDensity::Normal => 32.0,
    }
}

pub(super) fn alias_badge_size(density: DisplayDensity) -> (f64, f64) {
    match density {
        DisplayDensity::Compact => (28.0, 18.0),
        DisplayDensity::Normal => (32.0, 22.0),
    }
}

pub(super) fn panel_height(
    count: usize,
    switching: bool,
    extra_controls: bool,
    show_mode_label: bool,
    density: DisplayDensity,
) -> f64 {
    let header = if switching { 36.0 } else { HEIGHT - LIST_TOP };
    let footer = if extra_controls { 64.0 } else { LIST_BOTTOM }
        + if show_mode_label {
            MODE_LABEL_SPACING
        } else {
            0.0
        };
    let content = if count == 0 {
        184.0
    } else {
        count as f64 * row_height(density) + 8.0
    };
    (header + footer + content).clamp(280.0, HEIGHT)
}
pub(super) fn set_label(field: &NSTextField, text: &str) {
    if field.stringValue().to_string() != text {
        field.setStringValue(&NSString::from_str(text));
    }
}
pub(super) fn label(
    text: &str,
    size: f64,
    frame: NSRect,
    mtm: MainThreadMarker,
) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setFont(Some(&NSFont::systemFontOfSize(size)));
    label.setFrame(frame);
    label
}
