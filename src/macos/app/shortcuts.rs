use super::*;

impl Delegate {
    pub(super) fn register_hotkeys(&self) {
        let result = {
            let config = self.ivars().config.borrow();
            config.validate().and_then(|()| {
                ShortcutTap::new(
                    self.mtm(),
                    &config,
                    self.ivars().wake.get().unwrap().handle(),
                )
            })
        };
        match result {
            Ok((tap, receiver)) => {
                self.ivars().shortcut_tap.replace(Some(tap));
                self.ivars().shortcut_rx.replace(Some(receiver));
                self.ivars().hotkey_error.replace(None);
            }
            Err(error) => {
                self.ivars().hotkey_error.replace(Some(error));
            }
        }
    }

    pub(super) fn check_shortcuts(&self) {
        let state = self.ivars();
        if state
            .last_shortcut_check
            .get()
            .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        state.last_shortcut_check.set(Some(Instant::now()));
        if !accessibility::is_trusted() {
            state.focus_observer.replace(None);
            state.focus_receiver.replace(None);
            if state.shortcut_tap.borrow().is_some() {
                self.end_session();
                state.shortcut_tap.replace(None);
                state.shortcut_rx.replace(None);
                state.hotkey_error.replace(Some(
                    tr!(
                        "全局快捷键未启用，请在系统设置中重新允许 Winlane。",
                        "Global shortcuts are disabled. Allow Winlane again in System Settings."
                    )
                    .into(),
                ));
                self.report_shortcut_status();
            }
        } else if state.shortcut_tap.borrow().is_none() {
            self.register_hotkeys();
            self.track_frontmost();
            self.report_shortcut_status();
        } else if !state
            .shortcut_tap
            .borrow()
            .as_ref()
            .is_some_and(ShortcutTap::is_enabled)
        {
            state.hotkey_error.replace(Some(
                tr!(
                    "快捷键监听已暂停，请重新启动 Winlane。",
                    "Shortcut monitoring is paused. Restart Winlane."
                )
                .into(),
            ));
            self.report_shortcut_status();
        }
    }

    pub(super) fn report_shortcut_status(&self) {
        if let Some(settings) = self.settings_window() {
            if let Some(error) = self.ivars().hotkey_error.borrow().as_deref() {
                settings.report(error, true);
            } else {
                let config = self.ivars().config.borrow();
                settings.report(
                    &trf!(
                        "搜索 {} · 切换 {} 已启用。",
                        "Search {} · Switch {} enabled.",
                        config.shortcut.display(),
                        config.switch_shortcut.display()
                    ),
                    false,
                );
            }
        }
    }

    pub(super) fn drain_shortcut_actions(&self) {
        let actions: Vec<_> = self
            .ivars()
            .shortcut_rx
            .borrow()
            .as_ref()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();
        for action in actions {
            self.shortcut_action(action);
        }
    }

    pub(super) fn shortcut_action(&self, action: Action) {
        match action.kind {
            ActionKind::LaunchApp(index) => self.launch_app_shortcut(index),
            ActionKind::OpenQuicklink(id) => self.open_quicklink_shortcut(&id, action.session),
            ActionKind::RunCommand(command) => self.run_command_shortcut(command, action.session),
            ActionKind::Search => {
                if self.ivars().mode.get().is_some() && self.ivars().session.get() == action.session
                {
                    self.display_search(action.session);
                } else {
                    self.show_mode(PanelMode::Search, action.session, 0);
                }
            }
            ActionKind::Switch { direction, fresh } => {
                if fresh {
                    self.show_mode(PanelMode::Switch, action.session, direction);
                } else if self.ivars().session.get() == action.session {
                    self.move_selection(isize::from(direction));
                }
            }
            ActionKind::Accept if self.ivars().session.get() == action.session => {
                if let Some(selection) = self.ivars().switch_selection.borrow_mut().as_mut() {
                    selection.release();
                }
                self.commit_switch_if_ready();
            }
            ActionKind::Alias(ch)
                if self.ivars().session.get() == action.session
                    && self.ivars().mode.get() == Some(PanelMode::Switch) =>
            {
                self.ivars().alias_input.borrow_mut().push(ch);
                self.select_alias();
                self.render();
            }
            ActionKind::AliasBackspace
                if self.ivars().session.get() == action.session
                    && self.ivars().mode.get() == Some(PanelMode::Switch) =>
            {
                self.ivars().alias_input.borrow_mut().pop();
                self.select_alias();
                self.render();
            }
            ActionKind::Cancel if self.ivars().session.get() == action.session => {
                self.end_session();
            }
            _ => {}
        }
    }
}
