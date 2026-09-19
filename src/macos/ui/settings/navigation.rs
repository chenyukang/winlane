use super::*;

#[derive(Debug)]
pub(super) struct NavigationStyle {
    color: Retained<NSColor>,
    pub(super) symbol: Option<Retained<NSImage>>,
}

define_class!(
    // SAFETY: Settings navigation and drawing stay on AppKit's main thread.
    #[unsafe(super = NSButton)]
    #[thread_kind = MainThreadOnly]
    #[ivars = NavigationStyle]
    #[derive(Debug)]
    pub(super) struct SettingsNavigationButton;
    unsafe impl NSObjectProtocol for SettingsNavigationButton {}
    impl SettingsNavigationButton {
        #[unsafe(method(drawRect:))]
        fn draw(&self, dirty: NSRect) {
            if self.state() == NSControlStateValueOn || self.isHighlighted() {
                NSColor::labelColor().colorWithAlphaComponent(0.09).setFill();
                NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(self.bounds(), 9.0, 9.0).fill();
            }
            // SAFETY: The native button draws its title and keyboard focus indication.
            unsafe { let _: () = msg_send![super(self), drawRect: dirty]; }
            self.ivars().color.setFill();
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect(10.0, 7.0, 26.0, 26.0), 6.0, 6.0).fill();
            if let Some(symbol) = &self.ivars().symbol {
                // SAFETY: No drawing hints are provided; respect the native button's flipped coordinates.
                unsafe { symbol.drawInRect_fromRect_operation_fraction_respectFlipped_hints(rect(14.0, 11.0, 18.0, 18.0), NSRect::ZERO, NSCompositingOperation::SourceOver, 1.0, true, None); }
            }
        }
    }
);

impl SettingsNavigationButton {
    pub(super) fn new(
        title: &str,
        symbol: &str,
        color: Retained<NSColor>,
        frame: NSRect,
        target: &AnyObject,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let symbol = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(symbol),
            None,
        )
        .and_then(|image| {
            image.imageWithSymbolConfiguration(
                &NSImageSymbolConfiguration::configurationWithHierarchicalColor(
                    &NSColor::whiteColor(),
                ),
            )
        });
        let this = Self::alloc(mtm).set_ivars(NavigationStyle { color, symbol });
        // SAFETY: The button is initialized once with its main-thread-owned drawing state.
        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        this.setTitle(&NSString::from_str(&format!("           {title}")));
        this.setFont(Some(&NSFont::systemFontOfSize(14.0)));
        this.setAlignment(NSTextAlignment::Left);
        this.setBordered(false);
        this.setButtonType(NSButtonType::MomentaryChange);
        this.setAccessibilityLabel(Some(&NSString::from_str(title)));
        set_action(&this, target, sel!(selectSettingsSection:));
        this
    }
}
