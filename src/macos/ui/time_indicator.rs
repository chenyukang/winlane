use super::controls::{label, rect};
use super::input_indicator::{IndicatorPanel, Screen, display_screens, native_color};
use objc2::MainThreadOnly;
use objc2::rc::Retained;
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSDate, NSDateFormatter, NSDateFormatterStyle, NSString};
use winlane::features::input_indicator::TimeSettings;

struct Surface {
    id: u32,
    panel: Retained<IndicatorPanel>,
    background: Retained<NSBox>,
    text: Retained<NSTextField>,
}

impl Surface {
    fn new(id: u32, mtm: MainThreadMarker) -> Self {
        let panel = IndicatorPanel::new(mtm);
        let root = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 1.0, 1.0));
        panel.setContentView(Some(&root));
        let background = NSBox::initWithFrame(NSBox::alloc(mtm), root.bounds());
        background.setBoxType(NSBoxType::Custom);
        background.setTitlePosition(NSTitlePosition::NoTitle);
        background.setBorderWidth(0.0);
        root.addSubview(&background);
        let text = label("", 12.0, root.bounds(), mtm);
        text.setAlignment(NSTextAlignment::Center);
        text.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        text.setMaximumNumberOfLines(1);
        root.addSubview(&text);
        Self {
            id,
            panel,
            background,
            text,
        }
    }

    fn update(&self, settings: &TimeSettings, label: &str, screen: &Screen) {
        self.text.setStringValue(&NSString::from_str(label));
        self.text.setFont(Some(&NSFont::systemFontOfSize_weight(
            settings.size.font_size(),
            unsafe { NSFontWeightMedium },
        )));
        self.text.sizeToFit();
        let text_size = self.text.frame().size;
        let frame = settings.frame(screen.frame, screen.safe_area, text_size.width);
        self.panel
            .setFrame_display(rect(frame.x, frame.y, frame.width, frame.height), false);
        self.background
            .setFrame(rect(0.0, 0.0, frame.width, frame.height));
        self.background
            .setCornerRadius(frame.width.min(frame.height) / 2.0);
        self.background.setFillColor(&native_color(settings.color));
        self.text.setHidden(false);
        let text_color = if settings.color.dark_text() {
            NSColor::blackColor()
        } else {
            NSColor::whiteColor()
        };
        self.text.setTextColor(Some(&text_color));
        self.text.setFrame(rect(
            8.0_f64.min(frame.width / 4.0),
            ((frame.height - text_size.height) / 2.0).max(0.0),
            (frame.width - 16.0).max(1.0),
            text_size.height.min(frame.height),
        ));
        self.panel.setTitle(&NSString::from_str(label));
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        self.panel.orderOut(None);
        self.panel.close();
    }
}

#[derive(Default)]
pub(crate) struct Indicator {
    surfaces: Vec<Surface>,
}

impl Indicator {
    pub(crate) fn configure(
        &mut self,
        settings: &TimeSettings,
        screens: &[Screen],
        mtm: MainThreadMarker,
    ) {
        if !settings.enabled {
            self.surfaces.clear();
            return;
        }
        let label = current_time_label();
        let screens = display_screens(settings.display_target, screens, mtm);
        let mut ids = Vec::new();
        let mut frames = Vec::new();
        for screen in screens {
            if frames.contains(&screen.frame)
                || screen.frame.width <= 0.0
                || screen.frame.height <= 0.0
            {
                continue;
            }
            frames.push(screen.frame);
            ids.push(screen.id);
            if !self.surfaces.iter().any(|surface| surface.id == screen.id) {
                self.surfaces.push(Surface::new(screen.id, mtm));
            }
            self.surfaces
                .iter()
                .find(|surface| surface.id == screen.id)
                .unwrap()
                .update(settings, &label, screen);
        }
        self.surfaces.retain(|surface| ids.contains(&surface.id));
    }

    pub(crate) fn frames(&self) -> Vec<winlane::core::displays::Rect> {
        self.surfaces
            .iter()
            .map(|surface| {
                let frame = surface.panel.frame();
                winlane::core::displays::Rect {
                    x: frame.origin.x,
                    y: frame.origin.y,
                    width: frame.size.width,
                    height: frame.size.height,
                }
            })
            .collect()
    }

    pub(crate) fn show(&self) {
        for surface in &self.surfaces {
            surface.panel.orderFrontRegardless();
        }
    }
}

fn current_time_label() -> String {
    thread_local! {
        static FORMATTER: Retained<NSDateFormatter> = {
            let formatter = NSDateFormatter::new();
            formatter.setDateStyle(NSDateFormatterStyle::NoStyle);
            formatter.setTimeStyle(NSDateFormatterStyle::ShortStyle);
            formatter
        };
    }
    let now = NSDate::now();
    FORMATTER.with(|formatter| formatter.stringFromDate(&now).to_string())
}

#[cfg(test)]
#[path = "../../../tests/native/time_indicator.rs"]
pub(crate) mod tests;
