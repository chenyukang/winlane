use super::*;

impl Delegate {
    pub(super) fn show(&self) {
        self.cancel_routing();
        let action = self
            .ivars()
            .shortcut_tap
            .borrow()
            .as_ref()
            .map(ShortcutTap::open_search);
        let session = action.map_or_else(
            || self.ivars().session.get().wrapping_add(1),
            |action| action.session,
        );
        self.show_mode(PanelMode::Search, session, 0);
    }

    pub(super) fn open_switcher(&self) {
        let action = self
            .ivars()
            .shortcut_tap
            .borrow()
            .as_ref()
            .map(|tap| tap.enter_switch(NSEvent::modifierFlags_class().bits() as u64));
        if let Some(action) = action {
            self.shortcut_action(action);
        } else {
            self.show();
        }
    }

    pub(super) fn show_mode(&self, mode: PanelMode, session: u64, direction: i8) {
        let already_visible = self.any_panel_visible();
        self.prepare_panel(mode, session, direction);
        if mode == PanelMode::Switch && !already_visible {
            self.schedule_switch_panel();
        } else {
            self.present_panels();
        }
        self.schedule_cache_warmup();
    }

    pub(super) fn prepare_panel(&self, mode: PanelMode, session: u64, direction: i8) {
        self.prepare_panel_in_scope(mode, session, direction, None);
    }

    pub(super) fn prepare_panel_in_scope(
        &self,
        mode: PanelMode,
        session: u64,
        direction: i8,
        scope: Option<SearchScope>,
    ) {
        self.ivars().auto_appclose.borrow().cancel();
        self.cancel_settings_focus();
        self.ivars().project_open.take();
        self.remember_app_input();
        self.ivars().preparing_panel.set(true);
        self.cancel_scoped_refresh();
        self.clear_quicklink_input();
        self.ivars().search_scope.set(scope);
        if let Some(timer) = self.ivars().snippet_paste_timer.take() {
            timer.invalidate();
        }
        if let Some(form) = self.ivars().snippet_arguments.take() {
            form.window().close();
        }
        self.cancel_switch_timer();
        self.finish_search_input();
        // A new search intentionally discards the previous editing session;
        // ordinary result refreshes must preserve its uncommitted composition.
        for ui in self.panels() {
            ui.input.abortEditing();
            // A reused window remembers its last field editor. Keep it from
            // activating the previous IME before windowDidBecomeKey runs.
            if let Some(root) = ui.panel.contentView() {
                ui.panel.makeFirstResponder(Some(&root));
            }
        }
        if mode == PanelMode::Search {
            self.prepare_search_input();
        }
        self.ivars().launch_receiver.replace(None);
        if let Some(settings) = self.app_shortcuts_window() {
            settings.window.orderOut(None);
        }
        if let Some(settings) = self.settings_window() {
            settings.window.orderOut(None);
        }
        let preserve = self.selected_result();
        let preserve_window = self.selected_window().map(|window| window.id);
        let deferred = self.ivars().deferred_windows.borrow_mut().take();
        if let Some(windows) = deferred {
            self.install_windows(windows);
        }
        if mode == PanelMode::Switch {
            // A quick release can commit before any worker returns. Preserve
            // bounded exact capture for apps that omit focus notifications.
            self.remember_frontmost_window();
        } else {
            self.capture_panel_origin();
        }
        self.ivars().query.borrow_mut().clear();
        self.ivars().session.set(session);
        self.ivars().mode.set(Some(mode));
        self.ivars().alias_input.borrow_mut().clear();
        self.ivars()
            .switch_selection
            .replace((mode == PanelMode::Switch).then(|| SwitchSelection::new(direction)));
        self.ivars().switch_anchor.set(if direction == 0 {
            preserve_window
        } else {
            None
        });
        if mode == PanelMode::Switch {
            self.ivars().current_app_only.set(false);
        }
        self.schedule_scoped_refresh();
        self.filter_preserving(if scope.is_some() { None } else { preserve });
        self.prepare_switch_selection();
        if !self.ivars().demo.get() {
            self.refresh();
        }
        self.ivars().preparing_panel.set(false);
        self.sync_displays();
    }

    pub(super) fn cancel_switch_timer(&self) -> bool {
        if let Some(timer) = self.ivars().switch_timer.borrow_mut().take() {
            timer.invalidate();
            true
        } else {
            false
        }
    }

    pub(super) fn schedule_switch_panel(&self) {
        self.cancel_switch_timer();
        let delay = self.ivars().config.borrow().switch_delay_ms;
        if delay == 0 {
            self.present_panels();
            return;
        }
        // SAFETY: One-shot main-thread timer; invalidated on cancellation, mode
        // change, or confirmation. Identity checks reject stale callbacks.
        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                f64::from(delay) / 1000.0,
                self,
                sel!(presentSwitch:),
                None,
                false,
            )
        };
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
        self.ivars().switch_timer.replace(Some(timer));
    }

    pub(super) fn scoped_search(&self) -> bool {
        (self.ivars().search_scope.get().is_some() || self.editing_quicklink())
            && self.ivars().mode.get() == Some(PanelMode::Search)
    }

    pub(super) fn searching_keep_awake(&self) -> bool {
        self.scoped_search() && self.ivars().search_scope.get() == Some(SearchScope::KeepAwake)
    }

    pub(super) fn searching_bluetooth(&self) -> bool {
        self.scoped_search() && self.ivars().search_scope.get() == Some(SearchScope::Bluetooth)
    }

    pub(super) fn searching_open_url(&self) -> bool {
        self.scoped_search() && self.ivars().search_scope.get() == Some(SearchScope::OpenUrl)
    }

    pub(super) fn searching_projects(&self) -> bool {
        self.scoped_search() && self.ivars().search_scope.get() == Some(SearchScope::Projects)
    }

    pub(super) fn searching_quicklinks(&self) -> bool {
        self.scoped_search() && self.ivars().search_scope.get() == Some(SearchScope::Quicklinks)
    }

    pub(super) fn searching_snippets(&self) -> bool {
        self.scoped_search() && self.ivars().search_scope.get() == Some(SearchScope::Snippets)
    }

    pub(super) fn searching_emoji(&self) -> bool {
        self.scoped_search() && self.ivars().search_scope.get() == Some(SearchScope::Emoji)
    }

    pub(super) fn searching_clipboard(&self) -> bool {
        self.scoped_search() && self.ivars().search_scope.get() == Some(SearchScope::Clipboard)
    }

    pub(super) fn enter_scoped_search(&self, scope: SearchScope) {
        self.prepare_search_field();
        self.ivars().search_scope.set(Some(scope));
        self.ivars().query.borrow_mut().clear();
        self.schedule_scoped_refresh();
        self.filter();
        self.focus_search();
    }

    pub(super) fn cancel_scoped_refresh(&self) {
        if let Some(timer) = self.ivars().scoped_refresh_timer.take() {
            timer.invalidate();
        }
    }

    pub(super) fn schedule_scoped_refresh(&self) {
        self.cancel_scoped_refresh();
        let needs_refresh = (self.searching_bluetooth() && !self.bluetooth_busy())
            || (self.searching_projects() && self.ivars().project_receiver.borrow().is_none())
            || (self.searching_open_url() && self.ivars().open_url_receiver.borrow().is_none());
        if !needs_refresh {
            return;
        }
        // Yield to AppKit before starting discovery. Both command shortcuts and
        // in-panel entry render cached rows (or a loading state) on this turn.
        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                0.0,
                self,
                sel!(refreshSearchScope:),
                None,
                false,
            )
        };
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
        self.ivars().scoped_refresh_timer.replace(Some(timer));
    }

    pub(super) fn refresh_search_scope(&self, timer: &NSTimer) {
        if !self
            .ivars()
            .scoped_refresh_timer
            .borrow()
            .as_ref()
            .is_some_and(|pending| std::ptr::eq(&**pending, timer))
        {
            return;
        }
        self.cancel_scoped_refresh();
        if self.searching_bluetooth() {
            self.refresh_bluetooth();
        } else if self.searching_projects() {
            self.refresh_projects(false);
        } else if self.searching_open_url() {
            self.refresh_open_url();
        }
        self.render();
    }

    pub(super) fn leave_scoped_search(&self) {
        self.cancel_scoped_refresh();
        if let Some(input) = self.ivars().quicklink_input.take() {
            self.prepare_search_field();
            let id = input.link.id;
            self.clear_quicklink_input();
            self.filter_preserving(Some(SelectedResult::Quicklink(id)));
            self.focus_search();
            return;
        }
        if !self.scoped_search() {
            return;
        }
        self.prepare_search_field();
        let command = if self.searching_files() {
            CommandId::Files
        } else if self.searching_keep_awake() {
            CommandId::KeepAwake
        } else if self.searching_bluetooth() {
            CommandId::Bluetooth
        } else if self.searching_open_url() {
            CommandId::OpenUrl
        } else if self.searching_projects() {
            CommandId::Projects
        } else if self.searching_quicklinks() {
            CommandId::Quicklinks
        } else if self.searching_clipboard() {
            CommandId::Clipboard
        } else if self.searching_emoji() {
            CommandId::Emoji
        } else {
            CommandId::Snippets
        };
        self.ivars().search_scope.set(None);
        self.ivars().query.replace(command.definition().name.into());
        self.filter_preserving(Some(SelectedResult::Command(command)));
        self.focus_search();
    }

    pub(super) fn cancel_search(&self) {
        self.dismiss();
    }

    pub(super) fn dismiss(&self) {
        self.cancel_routing();
        self.end_session();
        if let Some(previous) = NSRunningApplication::runningApplicationWithProcessIdentifier(
            self.ivars().previous_pid.get(),
        ) {
            self.activate_app(&previous);
        }
    }

    pub(super) fn cancel_routing(&self) {
        if let Some(timer) = self.ivars().snippet_paste_timer.take() {
            timer.invalidate();
        }
        if let Some(tap) = self.ivars().shortcut_tap.borrow().as_ref() {
            tap.cancel();
        }
        if let Some(receiver) = self.ivars().shortcut_rx.borrow().as_ref() {
            for _ in receiver.try_iter() {}
        }
    }

    pub(super) fn end_session(&self) {
        self.cancel_settings_focus();
        self.ivars().project_open.take();
        self.cancel_scoped_refresh();
        self.clear_quicklink_input();
        let files = self.searching_files();
        self.cancel_files_search();
        let clipboard = self.searching_clipboard();
        let projects = self.searching_projects();
        let open_url = self.searching_open_url();
        let bluetooth = self.searching_bluetooth();
        self.ivars().bluetooth_matches.borrow_mut().clear();
        self.ivars().emoji_matches.borrow_mut().clear();
        self.clear_open_url_matches();
        self.ivars().search_scope.set(None);
        self.ivars().keep_awake_matches.borrow_mut().clear();
        if clipboard || projects || open_url || bluetooth || files {
            self.ivars().clipboard_matches.borrow_mut().clear();
            self.ivars().project_matches.borrow_mut().clear();
            for ui in self.panels() {
                for row in ui.rows.borrow_mut().drain(..) {
                    row.button.removeFromSuperview();
                }
            }
        }
        self.cancel_switch_timer();
        self.finish_search_input();
        let state = self.ivars();
        if state.mode.replace(None).is_none() {
            return;
        }
        if let Some(tap) = state.shortcut_tap.borrow().as_ref() {
            tap.finish(state.session.get());
        }
        state.switch_selection.replace(None);
        state.check_panel_focus.set(false);
        state.wake.get().unwrap().signal();
        for ui in self.panels() {
            // SAFETY: This main-thread AppKit action accepts a nil sender.
            unsafe { ui.project_progress.stopAnimation(None) };
            ui.project_progress.setHidden(true);
            ui.panel.orderOut(None);
        }
        let deferred = state.deferred_windows.borrow_mut().take();
        if let Some(windows) = deferred {
            self.install_windows(windows);
            self.filter();
        }
    }

    pub(super) fn toggle_mode(&self, flags: u64) {
        if self.scoped_search() {
            return;
        }
        if self.ivars().mode.get() == Some(PanelMode::Switch) {
            self.switch_to_search();
        } else {
            let action = self
                .ivars()
                .shortcut_tap
                .borrow()
                .as_ref()
                .map(|tap| tap.enter_switch(flags));
            if let Some(action) = action {
                self.shortcut_action(action);
            }
        }
    }

    pub(super) fn switch_to_search(&self) {
        let state = self.ivars();
        let action = state
            .shortcut_tap
            .borrow()
            .as_ref()
            .map(ShortcutTap::open_search);
        self.display_search(action.map_or(state.session.get(), |action| action.session));
    }

    pub(super) fn display_search(&self, session: u64) {
        self.clear_quicklink_input();
        if self.ivars().search_scope.replace(None).is_some() {
            self.ivars().query.borrow_mut().clear();
        }
        let was_delayed = self.cancel_switch_timer();
        if self.ivars().mode.get() != Some(PanelMode::Search) {
            self.prepare_search_input();
        }
        let state = self.ivars();
        let selected_id = self.selected_result();
        state.session.set(session);
        state.mode.set(Some(PanelMode::Search));
        state.alias_input.borrow_mut().clear();
        state.switch_selection.replace(None);
        let deferred = state.deferred_windows.borrow_mut().take();
        if let Some(windows) = deferred {
            self.install_windows(windows);
        }
        self.filter_preserving(selected_id);
        if was_delayed {
            self.present_panels();
        }
        self.focus_search();
    }

    pub(super) fn prepare_switch_selection(&self) {
        let state = self.ivars();
        if state.mode.get() != Some(PanelMode::Switch) {
            return;
        }
        let matches = state.matches.borrow();
        let windows = state.windows.borrow();
        let anchor = if let Some(id) = state.switch_anchor.get() {
            matches.iter().position(|&index| windows[index].id == id)
        } else {
            state
                .previous_window
                .get()
                .and_then(|id| matches.iter().position(|&index| windows[index].id == id))
                .or_else(|| {
                    matches
                        .iter()
                        .position(|&index| windows[index].pid == state.previous_pid.get())
                })
        };
        if let Some(selection) = state.switch_selection.borrow_mut().as_mut() {
            selection.install(matches.len(), anchor);
            if let Some(index) = selection.selected() {
                state.selected.set(index);
            }
        }
        drop(windows);
        drop(matches);
        self.select_alias();
        self.render();
    }

    pub(super) fn alias_position(&self) -> Option<usize> {
        self.alias_match().position()
    }

    pub(super) fn alias_match(&self) -> AliasMatch {
        let state = self.ivars();
        let windows = state.windows.borrow();
        let ordered_windows: Vec<_> = state
            .matches
            .borrow()
            .iter()
            .map(|&index| windows[index].id)
            .collect();
        state
            .aliases
            .borrow()
            .match_windows(state.alias_input.borrow().text(), &ordered_windows)
    }

    pub(super) fn select_alias(&self) {
        if let Some(index) = self.alias_position() {
            if let Some(selection) = self.ivars().switch_selection.borrow_mut().as_mut() {
                selection.select(index);
            }
            self.ivars().selected.set(index);
        }
    }

    pub(super) fn commit_switch_if_ready(&self) {
        let state = self.ivars();
        if state.mode.get() != Some(PanelMode::Switch) {
            return;
        }
        if !state.alias_input.borrow().text().is_empty() && self.alias_position().is_none() {
            let released = state
                .switch_selection
                .borrow()
                .as_ref()
                .is_some_and(SwitchSelection::released);
            if released && !state.loading.get() {
                self.end_session();
            }
            return;
        }
        let commit = state
            .switch_selection
            .borrow_mut()
            .as_mut()
            .and_then(SwitchSelection::take_commit);
        if let Some(index) = commit {
            state.selected.set(index);
            self.activate_selected();
        } else if !state.loading.get()
            && state
                .switch_selection
                .borrow()
                .as_ref()
                .is_some_and(SwitchSelection::released)
        {
            self.end_session();
        }
    }
}
