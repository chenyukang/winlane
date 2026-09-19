use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};
use winlane::tr;

define_class!(
    // SAFETY: Settings windows and event dispatch stay on AppKit's main thread.
    #[unsafe(super = NSWindow)]
    #[thread_kind = MainThreadOnly]
    #[ivars = super::input::WindowInput]
    pub(super) struct PreferencesWindow;
    unsafe impl NSObjectProtocol for PreferencesWindow {}
    impl PreferencesWindow {
        #[unsafe(method(becomeKeyWindow))]
        fn become_key(&self) {
            let input = self.ivars();
            if input.changing.replace(true) {
                unsafe { let _: () = msg_send![super(self), becomeKeyWindow]; }
                return;
            }
            input.prepare(self, self.mtm());
            let responder = self.firstResponder();
            input.configure(responder.as_deref(), false);
            if !super::input::composing(self) { input.select(self.mtm()); }
            unsafe { let _: () = msg_send![super(self), becomeKeyWindow]; }
            input.configure(responder.as_deref(), true);
            input.activate(self, self.mtm());
            input.changing.set(false);
        }
        #[unsafe(method(resignKeyWindow))]
        fn resign_key(&self) {
            let input = self.ivars();
            let changing = input.changing.replace(true);
            input.finish(self.mtm());
            unsafe { let _: () = msg_send![super(self), resignKeyWindow]; }
            input.changing.set(changing);
        }
        #[unsafe(method(makeFirstResponder:))]
        fn make_first_responder(&self, responder: Option<&NSResponder>) -> bool {
            let input = self.ivars();
            let same = responder.zip(self.firstResponder().as_deref())
                .is_some_and(|(next, current)| std::ptr::eq(next, current)
                    || next.downcast_ref::<NSControl>()
                        .and_then(|control| control.currentEditor())
                        .is_some_and(|editor| {
                            let editor: &NSResponder = &editor;
                            std::ptr::eq(editor, current)
                        }));
            if input.changing.get() || same || !super::input::editable(responder) {
                return unsafe { msg_send![super(self), makeFirstResponder: responder] };
            }
            input.changing.set(true);
            input.prepare(self, self.mtm());
            input.configure(responder, false);
            let composing = super::input::composing(self);
            if self.isKeyWindow() && !composing { input.select(self.mtm()); }
            let accepted: bool = unsafe { msg_send![super(self), makeFirstResponder: responder] };
            input.configure(responder, true);
            if accepted {
                if self.isKeyWindow() && composing && !super::input::composing(self) { input.select(self.mtm()); }
                input.activate(self, self.mtm());
            }
            input.changing.set(false);
            accepted
        }
        #[unsafe(method(sendEvent:))]
        fn send_event(&self, event: &NSEvent) {
            if event.r#type() == NSEventType::KeyDown
                && i64::from(event.keyCode()) == winlane::core::shortcuts::ESCAPE
                && !event.modifierFlags().intersects(
                    NSEventModifierFlags::Command | NSEventModifierFlags::Control
                        | NSEventModifierFlags::Option | NSEventModifierFlags::Shift,
                )
                && self.attachedSheet().is_none()
            {
                let composing = self.firstResponder()
                    .and_then(|responder| responder.downcast::<NSTextView>().ok())
                    .is_some_and(|editor| NSTextInputClient::hasMarkedText(&*editor));
                if !composing {
                    if !event.isARepeat() { self.performClose(None); }
                    return;
                }
            }
            if self.isKeyWindow() && event.r#type() == NSEventType::KeyDown {
                self.ivars().remember(self.mtm());
                if self.ivars().insert_layout_key(self, event, self.mtm()) { return; }
            }
            // SAFETY: Other keys and input-method cancellation keep their native behavior.
            unsafe { let _: () = msg_send![super(self), sendEvent: event]; }
            if self.isKeyWindow() && matches!(event.r#type(), NSEventType::KeyDown | NSEventType::FlagsChanged) {
                self.ivars().remember(self.mtm());
            }
        }
    }
);

pub(crate) fn preferences_window(frame: NSRect, mtm: MainThreadMarker) -> Retained<NSWindow> {
    // SAFETY: The initialized main-thread window is retained by its settings owner across closes.
    unsafe {
        let window: Retained<PreferencesWindow> = msg_send![
            super(PreferencesWindow::alloc(mtm).set_ivars(super::input::WindowInput::default())),
            initWithContentRect: frame,
            styleMask: NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ];
        window.setReleasedWhenClosed(false);
        window.into_super()
    }
}

pub(crate) fn card(parent: &NSView, frame: NSRect, mtm: MainThreadMarker) -> Retained<NSView> {
    let view = NSView::initWithFrame(NSView::alloc(mtm), frame);
    let background = NSBox::initWithFrame(NSBox::alloc(mtm), view.bounds());
    background.setBoxType(NSBoxType::Custom);
    background.setTitlePosition(NSTitlePosition::NoTitle);
    background.setCornerRadius(10.0);
    background.setBorderWidth(1.0);
    // Native group colors keep existing cards adaptive when the appearance changes.
    background.setBorderColor(&NSColor::separatorColor());
    background.setFillColor(&NSColor::quaternarySystemFillColor());
    background.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    view.addSubview(&background);
    parent.addSubview(&view);
    view
}

pub(crate) fn settings_group(
    parent: &NSView,
    title: &str,
    top: f64,
    height: f64,
    mtm: MainThreadMarker,
) -> Retained<NSView> {
    let heading = hint(title, rect(6.0, top - 22.0, 728.0, 22.0), mtm);
    heading.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
    parent.addSubview(&heading);
    card(parent, rect(0.0, top - 32.0 - height, 740.0, height), mtm)
}

pub(crate) fn row_text(
    parent: &NSView,
    title: &str,
    description: &str,
    top: f64,
    width: f64,
    mtm: MainThreadMarker,
) {
    parent.addSubview(&label(
        title,
        14.0,
        rect(20.0, top - 33.0, width, 24.0),
        mtm,
    ));
    parent.addSubview(&hint(description, rect(20.0, top - 72.0, width, 38.0), mtm));
}

pub(crate) fn editor_chrome(parent: &NSView, mtm: MainThreadMarker) {
    for frame in [rect(0.0, 0.0, 204.0, 574.0), rect(220.0, 0.0, 520.0, 574.0)] {
        let background = card(parent, frame, mtm);
        parent.addSubview_positioned_relativeTo(&background, NSWindowOrderingMode::Below, None);
    }
    let heading = hint(
        tr!("内容列表", "LIBRARY"),
        rect(16.0, 538.0, 174.0, 22.0),
        mtm,
    );
    heading.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
    parent.addSubview(&heading);
}

pub(crate) fn row_divider(parent: &NSView, y: f64, mtm: MainThreadMarker) {
    let line = NSBox::initWithFrame(
        NSBox::alloc(mtm),
        rect(20.0, y, parent.bounds().size.width - 40.0, 1.0),
    );
    line.setBoxType(NSBoxType::Separator);
    parent.addSubview(&line);
}

pub(crate) fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}
pub(crate) fn label(
    text: &str,
    size: f64,
    frame: NSRect,
    mtm: MainThreadMarker,
) -> Retained<NSTextField> {
    let value = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    value.setFont(Some(&NSFont::systemFontOfSize(size)));
    value.setFrame(frame);
    value
}
pub(crate) fn hint(text: &str, frame: NSRect, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let value = NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm);
    value.setFont(Some(&NSFont::systemFontOfSize(12.0)));
    value.setFrame(frame);
    value.setTextColor(Some(&NSColor::secondaryLabelColor()));
    value.setMaximumNumberOfLines(2);
    value
}
pub(crate) fn checkbox(title: &str, mtm: MainThreadMarker) -> Retained<NSButton> {
    let control = NSButton::new(mtm);
    control.setButtonType(NSButtonType::Switch);
    control.setTitle(&NSString::from_str(title));
    control
}
pub(crate) fn popup(
    titles: &[&str],
    frame: NSRect,
    mtm: MainThreadMarker,
) -> Retained<NSPopUpButton> {
    let control = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), frame, false);
    for title in titles {
        control.addItemWithTitle(&NSString::from_str(title));
    }
    control
}
pub(crate) fn set_action(button: &NSControl, target: &AnyObject, action: Sel) {
    // SAFETY: The application delegate outlives the controls and implements each selector.
    unsafe {
        button.setTarget(Some(target));
        button.setAction(Some(action));
    }
}
pub(crate) fn button(
    title: &str,
    target: &AnyObject,
    action: Sel,
    frame: NSRect,
    mtm: MainThreadMarker,
) -> Retained<NSButton> {
    let button = NSButton::initWithFrame(NSButton::alloc(mtm), frame);
    button.setTitle(&NSString::from_str(title));
    button.setBezelStyle(NSBezelStyle::Push);
    set_action(&button, target, action);
    button
}

pub(crate) fn input(
    frame: objc2_foundation::NSRect,
    placeholder: &str,
    mtm: MainThreadMarker,
) -> Retained<NSTextField> {
    let field = NSTextField::initWithFrame(NSTextField::alloc(mtm), frame);
    field.setFont(Some(&NSFont::systemFontOfSize(14.0)));
    field.setPlaceholderString(Some(&NSString::from_str(placeholder)));
    field.setAccessibilityLabel(Some(&NSString::from_str(placeholder)));
    field
}

pub(crate) fn text_area(
    frame: objc2_foundation::NSRect,
    editable: bool,
    mtm: MainThreadMarker,
) -> (Retained<NSScrollView>, Retained<NSTextView>) {
    let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), frame);
    scroll.setBorderType(NSBorderType::BezelBorder);
    scroll.setHasVerticalScroller(true);
    scroll.setAutohidesScrollers(true);
    let text = NSTextView::initWithFrame(
        NSTextView::alloc(mtm),
        rect(0.0, 0.0, frame.size.width - 16.0, frame.size.height),
    );
    text.setRichText(false);
    text.setImportsGraphics(false);
    text.setEditable(editable);
    text.setSelectable(true);
    text.setFont(Some(&NSFont::systemFontOfSize(14.0)));
    text.setTextContainerInset(NSSize::new(10.0, 10.0));
    text.setVerticallyResizable(true);
    text.setHorizontallyResizable(false);
    text.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
    text.setMaxSize(NSSize::new(frame.size.width - 16.0, f64::MAX));
    if let Some(container) = unsafe { text.textContainer() } {
        container.setWidthTracksTextView(true);
        container.setContainerSize(NSSize::new(frame.size.width - 16.0, f64::MAX));
    }
    text.setAutomaticQuoteSubstitutionEnabled(false);
    text.setAutomaticDashSubstitutionEnabled(false);
    text.setAutomaticTextReplacementEnabled(false);
    text.setAutomaticSpellingCorrectionEnabled(false);
    text.setContinuousSpellCheckingEnabled(false);
    scroll.setDocumentView(Some(&text));
    (scroll, text)
}
