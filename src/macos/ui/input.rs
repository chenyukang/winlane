use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2_app_kit::{
    NSEvent, NSResponder, NSTextField, NSTextFieldCell, NSTextInputClient, NSTextView, NSWindow,
};
use objc2_foundation::MainThreadMarker;
use winlane::core::input_method::InputSession;

use crate::macos::platform::input_source::{self, Preference, Source};

#[derive(Default)]
pub(super) struct WindowInput {
    pub changing: Cell<bool>,
    session: RefCell<InputSession>,
    preference: RefCell<Option<Preference>>,
}

impl WindowInput {
    pub fn prepare(&self, window: &NSWindow, mtm: MainThreadMarker) {
        if window.isKeyWindow() {
            self.remember(mtm);
        }
        let policy = input_source::policy(mtm);
        if !self.session.borrow().focused {
            self.session
                .borrow_mut()
                .prepare(Source::current(mtm).and_then(|source| source.id()), policy);
        }
        self.session.borrow_mut().set_policy(policy);
        self.preference
            .replace(Some(Preference::resolve(policy, mtm)));
    }

    pub fn select(&self, mtm: MainThreadMarker) {
        let target = self
            .preference
            .borrow()
            .as_ref()
            .and_then(|p| p.target.clone());
        if let Some(source) = target {
            source.select(mtm);
        }
    }

    pub fn configure(&self, responder: Option<&NSResponder>, clear: bool) {
        let composing = responder.is_some_and(|responder| {
            responder
                .downcast_ref::<NSTextView>()
                .is_some_and(NSTextInputClient::hasMarkedText)
                || responder
                    .downcast_ref::<NSTextField>()
                    .and_then(|field| field.currentEditor())
                    .and_then(|editor| editor.downcast::<NSTextView>().ok())
                    .is_some_and(|editor| NSTextInputClient::hasMarkedText(&*editor))
        });
        if composing {
            return;
        }
        let locales = self
            .preference
            .borrow()
            .as_ref()
            .and_then(|p| p.locales.clone())
            .filter(|_| !clear);
        if let Some(field) = responder.and_then(|r| r.downcast_ref::<NSTextField>())
            && let Some(cell) = field
                .cell()
                .and_then(|c| c.downcast::<NSTextFieldCell>().ok())
        {
            if cell.allowedInputSourceLocales().as_deref() != locales.as_deref() {
                cell.setAllowedInputSourceLocales(locales.as_deref());
            }
        } else if let Some(editor) = responder.and_then(|r| r.downcast_ref::<NSTextView>())
            && editor.allowedInputSourceLocales().as_deref() != locales.as_deref()
        {
            editor.setAllowedInputSourceLocales(locales.as_deref());
        }
    }

    pub fn activate(&self, window: &NSWindow, mtm: MainThreadMarker) {
        if !window.isKeyWindow() {
            return;
        }
        if let Some(editor) = editor(window)
            && !NSTextInputClient::hasMarkedText(&*editor)
        {
            let target = self
                .preference
                .borrow()
                .as_ref()
                .and_then(|p| p.target.clone());
            editor.setAllowedInputSourceLocales(None);
            input_source::align_editor(target.as_ref(), &editor);
        }
        self.session.borrow_mut().focused = true;
        self.remember(mtm);
    }

    pub fn remember(&self, mtm: MainThreadMarker) {
        let current = Source::current(mtm).and_then(|source| source.id());
        let mut session = self.session.borrow_mut();
        if session.observe(current)
            && let Some(id) = &session.selected
        {
            input_source::remember(id);
        }
    }

    pub fn finish(&self, mtm: MainThreadMarker) {
        let current = Source::current(mtm).and_then(|source| source.id());
        let previous = self.session.borrow_mut().finish(current.as_deref());
        self.preference.take();
        if let Some(source) = previous.and_then(|id| Source::by_id(&id, mtm)) {
            source.select(mtm);
        }
    }

    pub fn insert_layout_key(
        &self,
        window: &NSWindow,
        event: &NSEvent,
        mtm: MainThreadMarker,
    ) -> bool {
        let layout = self
            .preference
            .borrow()
            .as_ref()
            .and_then(|p| p.layout.clone());
        layout.is_some()
            && layout == Source::current(mtm).and_then(|source| source.id())
            && editor(window)
                .is_some_and(|editor| input_source::insert_keyboard_layout_text(&editor, event))
    }
}

pub(super) fn editor(window: &NSWindow) -> Option<Retained<NSTextView>> {
    window
        .firstResponder()
        .and_then(|r| r.downcast::<NSTextView>().ok())
}

pub(super) fn composing(window: &NSWindow) -> bool {
    editor(window).is_some_and(|editor| NSTextInputClient::hasMarkedText(&*editor))
}

pub(super) fn editable(responder: Option<&NSResponder>) -> bool {
    responder.is_some_and(|r| {
        r.downcast_ref::<NSTextField>()
            .is_some_and(NSTextField::isEditable)
            || r.downcast_ref::<NSTextView>()
                .is_some_and(NSTextView::isEditable)
    })
}

#[cfg(test)]
#[path = "../../../tests/native/input_windows.rs"]
pub(crate) mod tests;
