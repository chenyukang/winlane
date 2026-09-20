use super::*;

define_class!(
    // SAFETY: This panel and its delegate are created and used only on the main thread.
    #[unsafe(super = NSPanel)]
    #[thread_kind = MainThreadOnly]
    #[derive(Debug)]
    pub(super) struct SearchPanel;
    unsafe impl NSObjectProtocol for SearchPanel {}
    impl SearchPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool { true }
        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main(&self) -> bool { false }
        #[unsafe(method(contextMenuKeyDown:))]
        fn context_menu_key_down(&self, event: &NSEvent) {
            let composing = self.firstResponder()
                .and_then(|responder| responder.downcast::<NSTextView>().ok())
                .is_some_and(|editor| NSTextInputClient::hasMarkedText(&*editor));
            let delegate: Option<Retained<Delegate>> = unsafe { msg_send![self, delegate] };
            if let Some(delegate) = delegate
                && (delegate.is_open_url_input_key(event, composing)
                    || delegate.is_files_open_key(event, composing))
            {
                // macOS routes Control-Return here before sendEvent:. Reuse that
                // path so submission stays behind any buffered input-source keys.
                self.sendEvent(event);
                return;
            }
            unsafe { let _: () = msg_send![super(self), contextMenuKeyDown: event]; }
        }
        #[unsafe(method(sendEvent:))]
        fn send_event(&self, event: &NSEvent) {
            if event.r#type() == NSEventType::KeyDown {
                if i64::from(event.keyCode()) != winlane::core::shortcuts::ESCAPE {
                    let delegate: Option<Retained<Delegate>> = unsafe { msg_send![self, delegate] };
                    if let Some(delegate) = delegate && delegate.buffer_search_key(event) { return; }
                }
                let composing = self.firstResponder()
                    .and_then(|responder| responder.downcast::<NSTextView>().ok())
                    .is_some_and(|editor| NSTextInputClient::hasMarkedText(&*editor));
                let delegate: Option<Retained<Delegate>> = unsafe { msg_send![self, delegate] };
                if let Some(delegate) = delegate && delegate.is_open_url_input_key(event, composing) {
                    delegate.submit_open_url(true);
                    return;
                }
                let delegate: Option<Retained<Delegate>> = unsafe { msg_send![self, delegate] };
                if let Some(delegate) = delegate && delegate.handle_files_key(event, composing) { return; }
                if i64::from(event.keyCode()) == SPACE {
                    let delegate: Option<Retained<Delegate>> = unsafe { msg_send![self, delegate] };
                    if let Some(delegate) = delegate {
                        let mode = delegate.ivars().mode.get().unwrap_or(PanelMode::Search);
                        let empty = delegate.ivars().query.borrow().is_empty();
                        if !delegate.scoped_search() && space_changes_mode(mode, empty, composing) {
                            if !event.isARepeat() { delegate.toggle_mode(event.modifierFlags().bits() as u64); }
                            return;
                        }
                    }
                }
                if !composing && event.modifierFlags().intersection(NSEventModifierFlags::Command | NSEventModifierFlags::Control | NSEventModifierFlags::Option | NSEventModifierFlags::Shift) == NSEventModifierFlags::Command {
                    let delegate: Option<Retained<Delegate>> = unsafe { msg_send![self, delegate] };
                    if let Some(delegate) = delegate && delegate.searching_clipboard() {
                        match event.keyCode() {
                            8 => { delegate.use_clipboard(false); return; }
                            51 => { delegate.delete_clipboard_entry(); return; }
                            _ => {}
                        }
                    }
                }
                if let Some(command) = panel_command(i64::from(event.keyCode()), event.modifierFlags().bits() as u64, composing) {
                    // SAFETY: build_ui installs our retained Delegate as this panel's sole delegate.
                    let delegate: Option<Retained<Delegate>> = unsafe { msg_send![self, delegate] };
                    if let Some(delegate) = delegate {
                        if delegate.editing_quicklink() {
                            match command {
                                PanelCommand::Accept => { delegate.activate_selected(); return; }
                                PanelCommand::Cancel => { delegate.cancel_search(); return; }
                                PanelCommand::Next | PanelCommand::Previous if i64::from(event.keyCode()) == winlane::core::shortcuts::TAB => { delegate.step_quicklink_argument(if command == PanelCommand::Next { 1 } else { -1 }); return; }
                                _ => {}
                            }
                        } else {
                            if command == PanelCommand::Next && i64::from(event.keyCode()) == winlane::core::shortcuts::TAB && delegate.begin_selected_quicklink() { return; }
                            match command {
                                PanelCommand::Next => delegate.move_selection(1),
                                PanelCommand::Previous => delegate.move_selection(-1),
                                PanelCommand::Accept => delegate.activate_selected(),
                                PanelCommand::Cancel => delegate.cancel_search(),
                            }
                            return;
                        }
                    }
                }
                if !composing && event.modifierFlags().contains(NSEventModifierFlags::Command)
                    && NSApplication::sharedApplication(self.mtm()).mainMenu()
                        .is_some_and(|menu| menu.performKeyEquivalent(event))
                { return; }
                let delegate: Option<Retained<Delegate>> = unsafe { msg_send![self, delegate] };
                if let Some(delegate) = delegate && delegate.insert_layout_key(self, event) { return; }
            }
            // SAFETY: Unhandled events, including IME composition, follow NSPanel's normal dispatch.
            unsafe { let _: () = msg_send![super(self), sendEvent: event]; }
        }
    }
);

define_class!(
    // SAFETY: This main-thread content view receives focus without activating
    // an input context, until the configured source is selected for the editor.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[derive(Debug)]
    pub(super) struct PanelContentView;
    unsafe impl NSObjectProtocol for PanelContentView {}
    impl PanelContentView {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool { true }
    }
);

define_class!(
    // SAFETY: NSView has no extra subclass requirements. Coordinates are flipped for list rows.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[derive(Debug)]
    pub(super) struct ListView;
    unsafe impl NSObjectProtocol for ListView {}
    impl ListView {
        #[unsafe(method(isFlipped))]
        fn flipped(&self) -> bool { true }
    }
);

define_class!(
    // SAFETY: Drawing and mouse tracking stay on AppKit's main thread.
    #[unsafe(super = NSButton)]
    #[thread_kind = MainThreadOnly]
    #[ivars = RowAppearance]
    #[derive(Debug)]
    pub(super) struct WindowRowButton;
    unsafe impl NSObjectProtocol for WindowRowButton {}
    impl WindowRowButton {
        #[unsafe(method(drawRect:))]
        fn draw(&self, _: NSRect) {
            let selected = self.ivars().selected.get();
            let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(self.bounds(), 7.0, 7.0);
            if selected {
                let bounds = self.bounds();
                let glass = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                    rect(bounds.origin.x + 0.5, bounds.origin.y + 0.5, bounds.size.width - 1.0, bounds.size.height - 1.0), 7.0, 7.0,
                );
                // Reuse the panel's blurred backdrop beneath a uniform translucent tint.
                NSColor::whiteColor().colorWithAlphaComponent(0.10).setFill();
                glass.fill();
                tint(0x76a1df, 0.26).setFill();
                glass.fill();
                NSColor::whiteColor().colorWithAlphaComponent(0.28).setStroke();
                glass.setLineWidth(1.0);
                glass.stroke();
            } else if self.ivars().hovered.get() || self.isHighlighted() {
                NSColor::systemBlueColor().colorWithAlphaComponent(0.09).setFill();
                path.fill();
            }
            if self.ivars().has_alias.get() {
                let (width, height) = alias_badge_size(self.ivars().density.get());
                let chip = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                    rect(8.0, (self.bounds().size.height - height) / 2.0, width, height), 5.0, 5.0,
                );
                if selected {
                    NSColor::whiteColor().colorWithAlphaComponent(0.19).setFill();
                } else {
                    NSColor::systemBlueColor().colorWithAlphaComponent(0.09).setFill();
                }
                chip.fill();
            }
        }
        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking(&self) {
            if let Some(area) = self.ivars().tracking.borrow_mut().take() { self.removeTrackingArea(&area); }
            // SAFETY: The tracking area belongs to this view, which implements both mouse callbacks.
            unsafe {
                let _: () = msg_send![super(self), updateTrackingAreas];
                let area = NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(), self.bounds(),
                    NSTrackingAreaOptions::MouseEnteredAndExited | NSTrackingAreaOptions::ActiveAlways | NSTrackingAreaOptions::InVisibleRect,
                    Some(self), None,
                );
                self.addTrackingArea(&area);
                self.ivars().tracking.replace(Some(area));
            }
        }
        #[unsafe(method(mouseEntered:))]
        fn entered(&self, _: &NSEvent) { self.ivars().hovered.set(true); NSView::setNeedsDisplay(self, true); }
        #[unsafe(method(mouseExited:))]
        fn exited(&self, _: &NSEvent) { self.ivars().hovered.set(false); NSView::setNeedsDisplay(self, true); }
        #[unsafe(method(viewDidMoveToWindow))]
        fn moved_to_window(&self) {
            unsafe { let _: () = msg_send![super(self), viewDidMoveToWindow]; }
            self.ivars().hovered.set(false);
        }
        #[unsafe(method(resetCursorRects))]
        fn reset_cursor_rects(&self) {
            // SAFETY: Preserve NSButton's cursor setup before adding the row's cursor.
            unsafe { let _: () = msg_send![super(self), resetCursorRects]; }
            if self.isEnabled() {
                self.addCursorRect_cursor(self.visibleRect(), &NSCursor::pointingHandCursor());
            }
        }
    }
);

#[derive(Debug, Default)]
pub(super) struct RowAppearance {
    pub(super) density: Cell<DisplayDensity>,
    pub(super) selected: Cell<bool>,
    pub(super) hovered: Cell<bool>,
    pub(super) has_alias: Cell<bool>,
    pub(super) tracking: RefCell<Option<Retained<NSTrackingArea>>>,
}

impl RowUi {
    pub(super) fn select(&mut self, selected: bool) {
        if self.selected == Some(selected) {
            return;
        }
        self.selected = Some(selected);
        self.button.setAccessibilitySelected(selected);
        self.button.ivars().selected.set(selected);
        self.button
            .ivars()
            .has_alias
            .set(!self.alias.stringValue().is_empty());
        NSView::setNeedsDisplay(&self.button, true);
        let text = NSColor::labelColor();
        let launching = matches!(
            self.content,
            Some(
                RowContent::File(_)
                    | RowContent::Application(_)
                    | RowContent::Command(_)
                    | RowContent::Snippet(_)
                    | RowContent::Quicklink(_)
                    | RowContent::Project(_)
                    | RowContent::OpenUrl(_)
                    | RowContent::Clipboard(..)
            )
        );
        let detail = if launching && !selected {
            NSColor::secondaryLabelColor()
        } else {
            text.clone()
        };
        self.title.setTextColor(Some(&detail));
        self.app.setTextColor(Some(&text));
        self.app.setAlphaValue(if selected { 1.0 } else { 0.80 });
        let alias = if selected {
            text
        } else if launching {
            NSColor::systemTealColor()
        } else {
            NSColor::systemBlueColor()
        };
        self.alias.setTextColor(Some(&alias));
    }
}
