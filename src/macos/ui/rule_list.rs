use super::controls::{rect, set_action};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{AnyThread, DefinedClass, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSObjectProtocol, NSRect, NSSize, NSString};
use std::cell::RefCell;

#[derive(Debug, Default)]
pub(crate) struct RuleListStyle {
    image: RefCell<Option<Retained<NSImage>>>,
}

define_class!(
    // SAFETY: Rule navigation and drawing stay on AppKit's main thread.
    #[unsafe(super = NSButton)]
    #[thread_kind = MainThreadOnly]
    #[ivars = RuleListStyle]
    #[derive(Debug)]
    pub(crate) struct RuleListButton;
    unsafe impl NSObjectProtocol for RuleListButton {}
    impl RuleListButton {
        #[unsafe(method(drawRect:))]
        fn draw(&self, dirty: NSRect) {
            if self.state() == NSControlStateValueOn || self.isHighlighted() {
                NSColor::labelColor().colorWithAlphaComponent(0.09).setFill();
                NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(self.bounds(), 7.0, 7.0).fill();
            }
            // SAFETY: The native button draws its title and focus indication.
            unsafe { let _: () = msg_send![super(self), drawRect: dirty]; }
            if let Some(image) = self.ivars().image.borrow().as_ref() {
                let frame = rect(9.0, (self.bounds().size.height - 24.0) / 2.0, 24.0, 24.0);
                // SAFETY: No drawing hints are provided; respect native flipped coordinates.
                unsafe { image.drawInRect_fromRect_operation_fraction_respectFlipped_hints(frame, NSRect::ZERO, NSCompositingOperation::SourceOver, 1.0, true, None); }
            }
        }
    }
);

impl RuleListButton {
    pub(crate) fn new(target: &AnyObject, action: Sel, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(RuleListStyle::default());
        // SAFETY: Initialize the main-thread-owned button once.
        let this: Retained<Self> =
            unsafe { msg_send![super(this), initWithFrame: rect(0.0, 0.0, 240.0, 40.0)] };
        this.setFont(Some(&NSFont::systemFontOfSize(13.0)));
        this.setAlignment(NSTextAlignment::Left);
        this.setBordered(false);
        this.setButtonType(NSButtonType::MomentaryChange);
        this.cell()
            .unwrap()
            .setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        set_action(&this, target, action);
        this
    }

    pub(crate) fn set_label(&self, title: &str) {
        self.setTitle(&NSString::from_str(&format!("           {title}")));
        self.setAccessibilityLabel(Some(&NSString::from_str(title)));
        self.setToolTip(Some(&NSString::from_str(title)));
    }

    pub(crate) fn set_application_icon(&self, path: &str) {
        let source = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(path));
        // Keep only a small raster, rather than the application's complete ICNS representations.
        let image = NSImage::initWithSize(NSImage::alloc(), NSSize::new(24.0, 24.0));
        #[allow(deprecated)]
        {
            image.lockFocus();
            source.drawInRect(rect(0.0, 0.0, 24.0, 24.0));
            image.unlockFocus();
        }
        self.ivars().image.replace(Some(image));
        NSView::setNeedsDisplay(self, true);
    }

    pub(crate) fn set_symbol(&self, name: &str) {
        self.ivars().image.replace(
            NSImage::imageWithSystemSymbolName_accessibilityDescription(
                &NSString::from_str(name),
                None,
            )
            .and_then(|image| {
                image.imageWithSymbolConfiguration(
                    &NSImageSymbolConfiguration::configurationWithHierarchicalColor(
                        &NSColor::labelColor(),
                    ),
                )
            }),
        );
        NSView::setNeedsDisplay(self, true);
    }

    pub(crate) fn set_selected(&self, selected: bool) {
        self.setState(if selected {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        NSView::setNeedsDisplay(self, true);
    }
}
