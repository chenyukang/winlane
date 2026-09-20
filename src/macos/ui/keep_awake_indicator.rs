use super::controls::{label, rect};
use super::input_indicator::{IndicatorPanel, Screen};
use objc2::MainThreadOnly;
use objc2::rc::Retained;
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSString};
use winlane::core::displays::Rect;
use winlane::features::keep_awake::{IndicatorStatus, indicator_frame};

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
        background.setCornerRadius(12.0);
        background.setFillColor(&NSColor::colorWithSRGBRed_green_blue_alpha(
            1.0, 0.79, 0.38, 0.94,
        ));
        root.addSubview(&background);
        let text = label("", 12.0, root.bounds(), mtm);
        text.setFont(Some(&NSFont::systemFontOfSize_weight(12.0, unsafe {
            NSFontWeightMedium
        })));
        text.setTextColor(Some(&NSColor::blackColor()));
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

    fn update(&self, status: IndicatorStatus, screen: &Screen, occupied: &[Rect]) {
        let label = NSString::from_str(&status.label());
        self.text.setStringValue(&label);
        self.text.sizeToFit();
        let text_size = self.text.frame().size;
        let frame = indicator_frame(screen.safe_area, text_size.width, occupied);
        self.panel
            .setFrame_display(rect(frame.x, frame.y, frame.width, frame.height), false);
        self.panel.setTitle(&label);
        self.background
            .setFrame(rect(0.0, 0.0, frame.width, frame.height));
        self.text.setFrame(rect(
            8.0_f64.min(frame.width / 4.0),
            ((frame.height - text_size.height) / 2.0).max(0.0),
            (frame.width - 16.0).max(1.0),
            text_size.height.min(frame.height),
        ));
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
    pub status: Option<IndicatorStatus>,
    surfaces: Vec<Surface>,
}

impl Indicator {
    pub(crate) fn configure(
        &mut self,
        status: IndicatorStatus,
        screens: &[Screen],
        occupied: &[Rect],
        mtm: MainThreadMarker,
    ) {
        self.status = Some(status);
        let mut ids = Vec::new();
        let mut frames = Vec::new();
        for screen in screens {
            if frames.contains(&screen.frame)
                || screen.safe_area.width <= 0.0
                || screen.safe_area.height <= 0.0
            {
                continue;
            }
            ids.push(screen.id);
            frames.push(screen.frame);
            if !self.surfaces.iter().any(|surface| surface.id == screen.id) {
                self.surfaces.push(Surface::new(screen.id, mtm));
            }
            self.surfaces
                .iter()
                .find(|surface| surface.id == screen.id)
                .unwrap()
                .update(status, screen, occupied);
        }
        self.surfaces.retain(|surface| ids.contains(&surface.id));
    }

    pub(crate) fn show(&self) {
        for surface in &self.surfaces {
            surface.panel.orderFrontRegardless();
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/native/keep_awake_indicator.rs"]
pub(crate) mod tests;
