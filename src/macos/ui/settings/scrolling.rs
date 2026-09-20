use super::*;
use winlane::features::scrolling::Settings;

pub(super) struct ScrollingPage {
    enabled: Retained<NSButton>,
    mouse_vertical: Retained<NSButton>,
    mouse_horizontal: Retained<NSButton>,
    trackpad_vertical: Retained<NSButton>,
    trackpad_horizontal: Retained<NSButton>,
    step: Retained<NSTextField>,
    status: Retained<NSTextField>,
}
impl ScrollingPage {
    pub(super) fn new(host: &NSView, target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let general = settings_group(host, tr!("滚动设置", "Scrolling"), 570.0, 82.0, mtm);
        let make = |parent: &NSView, title, x, y, width| {
            let control = checkbox(title, mtm);
            control.setFrame(rect(x, y, width, 26.0));
            set_action(&control, target, sel!(settingsChanged:));
            parent.addSubview(&control);
            control
        };
        let enabled = make(
            &general,
            tr!("启用自定义滚动", "Enable custom scrolling"),
            20.0,
            44.0,
            520.0,
        );
        let status = hint("", rect(22.0, 6.0, 568.0, 36.0), mtm);
        general.addSubview(&status);
        general.addSubview(&button(
            tr!("重试", "Retry"),
            target,
            sel!(retryScrolling:),
            rect(606.0, 24.0, 110.0, 30.0),
            mtm,
        ));
        let mouse = settings_group(host, tr!("鼠标", "Mouse"), 436.0, 144.0, mtm);
        let mouse_vertical = make(
            &mouse,
            tr!("反转垂直滚动", "Reverse vertical scrolling"),
            20.0,
            101.0,
            340.0,
        );
        let mouse_horizontal = make(
            &mouse,
            tr!("反转水平滚动", "Reverse horizontal scrolling"),
            375.0,
            101.0,
            340.0,
        );
        row_divider(&mouse, 87.0, mtm);
        row_text(
            &mouse,
            tr!("滚轮步长", "Wheel step"),
            tr!(
                "0 使用系统默认；1–100 调整单格滚动。",
                "0 uses the system default; 1–100 adjusts single wheel steps."
            ),
            82.0,
            565.0,
            mtm,
        );
        let step =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(600.0, 30.0, 116.0, 28.0));
        step.cell().unwrap().setSendsActionOnEndEditing(true);
        set_action(&step, target, sel!(settingsChanged:));
        mouse.addSubview(&step);
        let trackpad = settings_group(host, tr!("触控板", "Trackpad"), 240.0, 62.0, mtm);
        let trackpad_vertical = make(
            &trackpad,
            tr!("反转垂直滚动", "Reverse vertical scrolling"),
            20.0,
            18.0,
            340.0,
        );
        let trackpad_horizontal = make(
            &trackpad,
            tr!("反转水平滚动", "Reverse horizontal scrolling"),
            375.0,
            18.0,
            340.0,
        );
        host.addSubview(&hint(tr!("方向相对于 macOS 的“自然滚动”设置。步长不影响触控板或 Magic Mouse。启用前请退出 Scroll Reverser 等其他滚轮工具，以免叠加。", "Directions are relative to macOS Natural Scrolling. Step size leaves trackpad and Magic Mouse scrolling unchanged. Quit other scroll tools such as Scroll Reverser before enabling."), rect(16.0, 44.0, 708.0, 72.0), mtm));
        Self {
            enabled,
            mouse_vertical,
            mouse_horizontal,
            trackpad_vertical,
            trackpad_horizontal,
            step,
            status,
        }
    }
    pub(super) fn fill(&self, config: &Config) {
        let s = &config.scrolling;
        for (control, value) in [
            (&self.enabled, s.enabled),
            (&self.mouse_vertical, s.mouse_vertical),
            (&self.mouse_horizontal, s.mouse_horizontal),
            (&self.trackpad_vertical, s.trackpad_vertical),
            (&self.trackpad_horizontal, s.trackpad_horizontal),
        ] {
            control.setState(if value {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
        self.step
            .setStringValue(&NSString::from_str(&s.wheel_step.to_string()));
    }
    pub(super) fn read(&self, config: &mut Config) -> Result<(), String> {
        let on = |button: &NSButton| button.state() == NSControlStateValueOn;
        config.scrolling = Settings {
            enabled: on(&self.enabled),
            mouse_vertical: on(&self.mouse_vertical),
            mouse_horizontal: on(&self.mouse_horizontal),
            trackpad_vertical: on(&self.trackpad_vertical),
            trackpad_horizontal: on(&self.trackpad_horizontal),
            wheel_step: self
                .step
                .stringValue()
                .to_string()
                .trim()
                .parse()
                .map_err(|_| {
                    tr!(
                        "滚轮步长应为 0–100 的整数。",
                        "Wheel step must be an integer from 0 to 100."
                    )
                })?,
        };
        config.scrolling.validate()
    }
    pub(super) fn status(&self, text: &str, error: bool) {
        if self.status.stringValue().to_string() != text {
            self.status.setStringValue(&NSString::from_str(text));
            self.status.setToolTip(Some(&NSString::from_str(text)));
            let color = if error {
                NSColor::systemRedColor()
            } else {
                NSColor::secondaryLabelColor()
            };
            self.status.setTextColor(Some(&color));
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/native/settings_scrolling.rs"]
pub(crate) mod tests;
