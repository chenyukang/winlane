use super::*;

impl Delegate {
    pub(super) fn focus_search(&self) {
        if self.ivars().changing_input_source.replace(true) {
            return;
        }
        for ui in self
            .panels()
            .into_iter()
            .filter(|ui| ui.panel.isKeyWindow())
        {
            match self.ivars().mode.get() {
                Some(PanelMode::Search) => {
                    let bar = ui.quicklink_bar.borrow();
                    let active = self
                        .ivars()
                        .quicklink_input
                        .borrow()
                        .as_ref()
                        .map(|input| input.active);
                    let control: &NSControl = active
                        .and_then(|index| bar.control(index))
                        .unwrap_or(&ui.input);
                    self.configure_input_start(control);
                    let new_editor = control.currentEditor().is_none();
                    let first_focus = !self.ivars().input_session.borrow().focused;
                    let source =
                        if first_focus || self.ivars().input_gate.borrow().target().is_some() {
                            self.ivars().input_target.borrow().clone()
                        } else if new_editor {
                            self.ivars()
                                .input_session
                                .borrow()
                                .selected
                                .as_deref()
                                .and_then(|id| Source::by_id(id, self.mtm()))
                        } else {
                            None
                        };
                    // The panel initially focuses its non-text content view;
                    // only activate a field editor after choosing its source.
                    if let Some(source) = &source {
                        source.select(self.mtm());
                    }
                    if new_editor {
                        ui.panel.makeFirstResponder(Some(control));
                    }
                    let editor = control
                        .currentEditor()
                        .and_then(|editor| editor.downcast::<NSTextView>().ok());
                    if let Some(editor) = editor
                        && !NSTextInputClient::hasMarkedText(&*editor)
                    {
                        input_source::align_editor(source.as_ref(), &editor);
                        self.ivars().input_session.borrow_mut().focused = true;
                        self.start_input_gate_timer();
                        self.remember_search_input();
                    }
                }
                Some(PanelMode::Switch) => {
                    ui.panel.makeFirstResponder(None);
                }
                _ => {}
            }
        }
        self.ivars().changing_input_source.set(false);
        self.complete_input_start();
    }

    pub(super) fn prepare_search_input(&self) {
        let current = Source::current(self.mtm()).and_then(|source| source.id());
        let policy = self.ivars().config.borrow().input_method;
        self.ivars()
            .input_session
            .borrow_mut()
            .prepare(current, policy);
        self.prepare_search_field();
    }

    pub(super) fn prepare_search_field(&self) {
        self.remember_search_input();
        self.cancel_input_start();
        let policy = self.ivars().config.borrow().input_method;
        self.ivars().input_session.borrow_mut().set_policy(policy);
        let preference = input_source::Preference::resolve(policy, self.mtm());
        self.ivars().input_layout_source.replace(preference.layout);
        self.ivars().input_start_locales.replace(preference.locales);
        self.ivars()
            .input_gate
            .borrow_mut()
            .begin(preference.target.as_ref().and_then(Source::id));
        self.ivars().input_target.replace(preference.target);
        for ui in self.panels() {
            self.configure_input_start(&ui.input);
            for control in ui.quicklink_bar.borrow().controls() {
                self.configure_input_start(control);
            }
        }
    }

    pub(super) fn configure_input_start(&self, input: &NSControl) {
        if let Some(cell) = input
            .cell()
            .and_then(|cell| cell.downcast::<NSTextFieldCell>().ok())
        {
            // Hint the requested language when AppKit configures the editor.
            // Source selection itself happens before giving the editor focus.
            let locales = self.ivars().input_start_locales.borrow();
            if cell.allowedInputSourceLocales().as_deref() != locales.as_deref() {
                cell.setAllowedInputSourceLocales(locales.as_deref());
            }
        }
    }

    pub(super) fn start_input_gate_timer(&self) {
        let state = self.ivars();
        if state.input_gate.borrow().target().is_none()
            || state.input_start_timer.borrow().is_some()
        {
            return;
        }
        state
            .input_start_deadline
            .set(Some(Instant::now() + Duration::from_millis(250)));
        // This timer exists only while input-source activation is pending.
        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                0.005,
                self,
                sel!(finishSearchInputStart:),
                None,
                true,
            )
        };
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
        state.input_start_timer.replace(Some(timer));
    }

    pub(super) fn cancel_input_start(&self) {
        let state = self.ivars();
        if let Some(timer) = state.input_start_timer.take() {
            timer.invalidate();
        }
        state.input_start_deadline.set(None);
        state.input_gate.borrow_mut().begin(None);
        state.input_target.replace(None);
        self.clear_input_start_locales();
    }

    pub(super) fn clear_input_start_locales(&self) {
        let state = self.ivars();
        if state.input_start_locales.take().is_some() {
            let changing = state.changing_input_source.replace(true);
            for ui in self.panels() {
                self.configure_input_start(&ui.input);
                for control in ui.quicklink_bar.borrow().controls() {
                    self.configure_input_start(control);
                }
                if let Some(editor) = ui
                    .panel
                    .firstResponder()
                    .and_then(|responder| responder.downcast::<NSTextView>().ok())
                {
                    editor.setAllowedInputSourceLocales(None);
                }
            }
            state.changing_input_source.set(changing);
        }
    }

    pub(super) fn buffer_search_key(&self, event: &NSEvent) -> bool {
        let state = self.ivars();
        if state.mode.get() != Some(PanelMode::Search)
            || state.input_gate.borrow().target().is_none()
        {
            return false;
        }
        // Queue even the event which observes readiness, so it cannot overtake
        // earlier letters, Backspace, or Return while the source was changing.
        state.input_gate.borrow_mut().push(Retained::from(event));
        self.start_input_gate_timer();
        self.complete_input_start();
        true
    }

    pub(super) fn insert_layout_key(&self, panel: &SearchPanel, event: &NSEvent) -> bool {
        let state = self.ivars();
        if state.mode.get() != Some(PanelMode::Search)
            || state.input_gate.borrow().target().is_some()
        {
            return false;
        }
        let Some(expected) = state.input_layout_source.borrow().clone() else {
            return false;
        };
        if Source::current(self.mtm())
            .and_then(|source| source.id())
            .as_deref()
            != Some(expected.as_str())
        {
            return false;
        }
        let Some(editor) = panel
            .firstResponder()
            .and_then(|responder| responder.downcast::<NSTextView>().ok())
        else {
            return false;
        };
        insert_keyboard_layout_text(&editor, event)
    }

    pub(super) fn complete_input_start(&self) {
        let state = self.ivars();
        if state.changing_input_source.get()
            || state.changing_displays.get()
            || state.input_gate.borrow().target().is_none()
        {
            return;
        }
        if state.mode.get() != Some(PanelMode::Search) {
            self.cancel_input_start();
            return;
        }
        if !self.any_panel_key() {
            // Scoped commands prepare their input before presenting the panel.
            // Only cancel here when an already-focused search loses focus.
            if state.input_session.borrow().focused {
                self.cancel_input_start();
            }
            return;
        }
        let current = Source::current(self.mtm()).and_then(|source| source.id());
        let context = self
            .panels()
            .into_iter()
            .find(|ui| ui.panel.isKeyWindow())
            .and_then(|ui| ui.panel.firstResponder())
            .and_then(|responder| responder.downcast::<NSTextView>().ok())
            .and_then(|editor| editor.inputContext());
        let active_context = NSTextInputContext::currentInputContext(self.mtm());
        let editor_active = context
            .as_ref()
            .zip(active_context.as_ref())
            .is_some_and(|(editor, active)| std::ptr::eq(&**editor, &**active));
        let editor_source = context
            .and_then(|context| context.selectedKeyboardInputSource())
            .map(|id| id.to_string());
        let ready = current.as_deref().filter(|id| {
            state.input_session.borrow().focused
                && editor_active
                && Some(*id) == editor_source.as_deref()
        });
        if ready.is_some()
            && ready == state.input_gate.borrow().target()
            && state.input_start_locales.borrow().is_some()
        {
            // Removing the temporary constraint can change the input context.
            // Reconfirm the source without restrictions before releasing keys.
            self.clear_input_start_locales();
            self.focus_search();
            return;
        }
        let expired = state
            .input_start_deadline
            .get()
            .is_some_and(|deadline| Instant::now() >= deadline);
        let Some(events) = state.input_gate.borrow_mut().finish(ready, expired) else {
            return;
        };
        self.cancel_input_start();
        self.remember_search_input();
        let session = state.session.get();
        for event in events {
            if state.mode.get() != Some(PanelMode::Search) || state.session.get() != session {
                break;
            }
            let Some(ui) = self.panels().into_iter().find(|ui| ui.panel.isKeyWindow()) else {
                break;
            };
            ui.panel.sendEvent(&event);
        }
    }

    pub(super) fn remember_search_input(&self) {
        if self.ivars().mode.get() != Some(PanelMode::Search)
            || !self.any_panel_key()
            || !self.ivars().input_session.borrow().focused
        {
            return;
        }
        let current = Source::current(self.mtm()).and_then(|source| source.id());
        if self
            .ivars()
            .input_gate
            .borrow()
            .target()
            .is_some_and(|target| Some(target) != current.as_deref())
        {
            return;
        }
        let mut session = self.ivars().input_session.borrow_mut();
        if session.observe(current)
            && let Some(id) = &session.selected
        {
            input_source::remember(id);
        }
    }

    pub(super) fn finish_search_input(&self) {
        self.ivars().input_layout_source.take();
        self.remember_search_input();
        self.cancel_input_start();
        if !self.ivars().input_session.borrow().focused {
            self.ivars().input_session.borrow_mut().finish(None);
            return;
        }
        // Do not overwrite a destination app's choice after focus has already moved away.
        let current = self
            .any_panel_key()
            .then(|| Source::current(self.mtm()))
            .flatten()
            .and_then(|source| source.id());
        let previous = self
            .ivars()
            .input_session
            .borrow_mut()
            .finish(current.as_deref());
        if let Some(previous) = previous.and_then(|id| Source::by_id(&id, self.mtm())) {
            self.ivars().changing_input_source.set(true);
            previous.select(self.mtm());
            self.ivars().changing_input_source.set(false);
        }
    }
}

pub(super) use input_source::insert_keyboard_layout_text;
