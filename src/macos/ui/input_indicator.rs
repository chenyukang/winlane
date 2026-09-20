use super::controls::{label, rect};
use objc2::rc::Retained;
use objc2::{MainThreadOnly, define_class, msg_send};
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSNumber, NSObjectProtocol, NSRect, NSString, ns_string};
use winlane::core::displays::Rect;
use winlane::features::input_indicator::{Color, InputSource, Settings, Style};

define_class!(
    // SAFETY: Display-only panels are owned and updated exclusively on the main thread.
    #[unsafe(super = NSPanel)]
    #[thread_kind = MainThreadOnly]
    #[ivars = ()]
    pub(crate) struct IndicatorPanel;
    unsafe impl NSObjectProtocol for IndicatorPanel {}
    impl IndicatorPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool { false }
        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main(&self) -> bool { false }
    }
);

impl IndicatorPanel {
    pub(crate) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        // SAFETY: This owned, borderless panel never activates the app; retained
        // ownership replaces AppKit's release-on-close behavior.
        let panel: Retained<IndicatorPanel> = unsafe {
            msg_send![super(IndicatorPanel::alloc(mtm).set_ivars(())), initWithContentRect: rect(0.0, 0.0, 1.0, 1.0),
                styleMask: NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
                backing: NSBackingStoreType::Buffered, defer: false]
        };
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setOpaque(false);
        panel.setHasShadow(false);
        panel.setIgnoresMouseEvents(true);
        panel.setHidesOnDeactivate(false);
        panel.setExcludedFromWindowsMenu(true);
        panel.setAnimationBehavior(NSWindowAnimationBehavior::None);
        panel.setLevel(NSStatusWindowLevel + 1);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        panel
    }
}

pub(crate) struct Screen {
    pub id: u32,
    pub frame: Rect,
    pub safe_area: Rect,
}

pub(crate) fn screens(mtm: MainThreadMarker) -> Vec<Screen> {
    NSScreen::screens(mtm)
        .iter()
        .enumerate()
        .map(|(index, screen)| {
            let frame = screen.frame();
            let visible = screen.visibleFrame();
            let top = screen.safeAreaInsets().top;
            let safe_top = (frame.origin.y + frame.size.height - top)
                .min(visible.origin.y + visible.size.height);
            Screen {
                id: screen
                    .deviceDescription()
                    .objectForKey(ns_string!("NSScreenNumber"))
                    .and_then(|v| v.downcast::<NSNumber>().ok())
                    .map_or(index as u32, |v| v.unsignedIntValue()),
                frame: from_rect(frame),
                safe_area: Rect {
                    x: visible.origin.x,
                    y: visible.origin.y,
                    width: visible.size.width,
                    height: (safe_top - visible.origin.y).max(1.0),
                },
            }
        })
        .collect()
}

fn from_rect(rect: NSRect) -> Rect {
    Rect {
        x: rect.origin.x,
        y: rect.origin.y,
        width: rect.size.width,
        height: rect.size.height,
    }
}

pub(crate) fn native_color(color: Color) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(
        f64::from(color.0) / 255.0,
        f64::from(color.1) / 255.0,
        f64::from(color.2) / 255.0,
        1.0,
    )
}

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
        let text = label("", 13.0, root.bounds(), mtm);
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

    fn update(&self, settings: &Settings, source: &InputSource, screen: &Screen) {
        let badge = settings.style == Style::Badge;
        let color = settings.color(source);
        self.text.setStringValue(&NSString::from_str(&source.name));
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
        self.background.setCornerRadius(match settings.style {
            Style::Bar => 0.0,
            Style::Badge | Style::Circle => frame.width.min(frame.height) / 2.0,
            Style::RoundedRectangle => frame.width.min(frame.height) / 4.0,
        });
        self.background.setFillColor(&native_color(color));
        self.text.setHidden(!badge);
        let text_color = if color.dark_text() {
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
        self.panel.setTitle(&NSString::from_str(&source.name));
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
        settings: &Settings,
        source: Option<&InputSource>,
        screens: &[Screen],
        mtm: MainThreadMarker,
    ) {
        let source = source
            .filter(|source| settings.enabled && !settings.hidden_sources.contains(&source.id));
        let Some(source) = source else {
            self.surfaces.clear();
            return;
        };
        let mut ids = Vec::new();
        let mut frames = Vec::new();
        for screen in screens
            .iter()
            .take(if settings.all_displays { usize::MAX } else { 1 })
        {
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
                .update(settings, source, screen);
        }
        self.surfaces.retain(|surface| ids.contains(&surface.id));
    }

    pub(crate) fn frames(&self) -> Vec<Rect> {
        self.surfaces
            .iter()
            .map(|surface| from_rect(surface.panel.frame()))
            .collect()
    }

    pub(crate) fn show(&self) {
        for surface in &self.surfaces {
            surface.panel.orderFrontRegardless();
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/native/input_indicator.rs"]
pub(crate) mod tests;
