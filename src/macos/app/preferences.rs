use super::*;
use crate::macos::platform::preferences as preference_store;

impl Delegate {
    pub(super) fn settings_window(&self) -> Option<Rc<SettingsWindow>> {
        self.ivars().settings.borrow().clone()
    }

    pub(super) fn ensure_settings_window(&self) -> Rc<SettingsWindow> {
        if let Some(window) = self.settings_window() {
            return window;
        }
        let window = Rc::new(SettingsWindow::new(self, self.mtm()));
        window
            .window
            .setDelegate(Some(ProtocolObject::from_ref(self)));
        self.ivars().settings.replace(Some(window.clone()));
        window.embed_snippets(self.ensure_snippet_editor().view());
        window.embed_quicklinks(self.ensure_quicklink_editor().view());
        self.update_update_settings();
        window
    }

    pub(super) fn prepare_update_ui(&self) {
        self.ivars().launch_receiver.replace(None);
        self.cancel_routing();
        self.end_session();
        NSApplication::sharedApplication(self.mtm()).activate();
    }

    pub(super) fn update_update_settings(&self) {
        if let Some(settings) = self.settings_window() {
            let updater = self.ivars().updater.get();
            settings.update_updater(
                updater.is_some(),
                updater.is_some_and(crate::macos::platform::updater::Updater::automatic_checks),
                self.ivars().updater_error.borrow().as_deref(),
            );
        }
    }

    pub(super) fn ensure_alias_rules_window(
        &self,
    ) -> Rc<crate::macos::ui::alias_rules::AliasRulesWindow> {
        if let Some(window) = self.ivars().alias_rules.borrow().clone() {
            return window;
        }
        let window = Rc::new(crate::macos::ui::alias_rules::AliasRulesWindow::new(
            self,
            self.mtm(),
        ));
        window
            .window
            .setDelegate(Some(ProtocolObject::from_ref(self)));
        self.ivars().alias_rules.replace(Some(window.clone()));
        window
    }

    pub(super) fn autosave_alias_rules(&self) {
        let Some(window) = self.ivars().alias_rules.borrow().clone() else {
            return;
        };
        let result = window.candidate().and_then(|rules| {
            let mut config = self.ivars().config.borrow().clone();
            config.alias_rules = rules;
            self.apply_config(config)
        });
        match result {
            Ok(()) => window.report(
                tr!(
                    "完整规则已自动保存；未填完的行暂不启用。",
                    "Complete rules saved automatically; unfinished rows stay inactive."
                ),
                false,
            ),
            Err(error) => window.report(&trf!("未保存：{}", "Not saved: {}", error), true),
        }
    }

    pub(super) fn app_shortcuts_window(&self) -> Option<Rc<AppShortcutsWindow>> {
        self.ivars().app_shortcuts.borrow().clone()
    }

    pub(super) fn ensure_app_shortcuts_window(&self) -> Rc<AppShortcutsWindow> {
        if let Some(window) = self.app_shortcuts_window() {
            return window;
        }
        let window = Rc::new(AppShortcutsWindow::new(self, self.mtm()));
        self.ivars().app_shortcuts.replace(Some(window.clone()));
        window
    }

    pub(super) fn check_language(&self) {
        let changed = preference_store::apply_language(self.ivars().config.borrow().language);
        if changed && self.ivars().status_item.get().is_some() {
            self.rebuild_localized_ui();
        }
    }

    pub(super) fn rebuild_localized_ui(&self) {
        let state = self.ivars();
        self.build_menus();
        let visible = self.any_panel_visible();
        state.changing_displays.set(true);
        for ui in state.panels.take() {
            ui.panel.setDelegate(None);
            ui.panel.orderOut(None);
            ui.panel.close();
        }
        state.changing_displays.set(false);
        self.sync_displays();
        self.render();
        if visible {
            self.present_panels();
        }
        let old_quicklink_editor = state.quicklink_editor.take();
        let old_snippet_editor = state.snippet_editor.take();
        let old_settings = state.settings.take();
        if let Some(old) = old_settings {
            let showing = old.window.isVisible();
            let frame = old.window.frame();
            let tab = old.selected_tab();
            let candidate = old
                .candidate()
                .unwrap_or_else(|_| state.config.borrow().clone());
            old.window.close();
            if showing {
                let window = self.ensure_settings_window();
                window.show(&candidate);
                window.select_tab(tab);
                window.window.setFrameOrigin(frame.origin);
                self.report_shortcut_status();
            }
        }
        if let Some(old) = old_quicklink_editor {
            self.ensure_quicklink_editor().copy_draft_from(&old);
        }
        if let Some(old) = old_snippet_editor {
            self.ensure_snippet_editor().copy_draft_from(&old);
        }
        let old_alias_rules = state.alias_rules.take();
        if let Some(old) = old_alias_rules {
            let showing = old.window.isVisible();
            let frame = old.window.frame();
            old.window.close();
            if showing {
                let window = self.ensure_alias_rules_window();
                window.copy_draft_from(&old, self, self.mtm());
                window.window.setFrameOrigin(frame.origin);
                window.window.makeKeyAndOrderFront(None);
            }
        }
        let old_shortcuts = state.app_shortcuts.take();
        if let Some(old) = old_shortcuts {
            let showing = old.window.isVisible();
            let frame = old.window.frame();
            old.window.close();
            if showing {
                let window = self.ensure_app_shortcuts_window();
                window.show(&[], self, self.mtm());
                window.copy_draft_from(&old, self, self.mtm());
                window.window.setFrameOrigin(frame.origin);
            }
        }
    }

    pub(super) fn autosave_settings(&self) {
        let Some(settings) = self.settings_window() else {
            return;
        };
        if self.ivars().saving_settings.replace(true) {
            return;
        }
        let result = settings
            .candidate()
            .and_then(|candidate| self.apply_config(candidate));
        self.ivars().saving_settings.set(false);
        if let Some(settings) = self.settings_window() {
            match result {
                Ok(()) => settings.report(tr!("已自动保存。", "Saved automatically."), false),
                Err(error) => settings.report(&trf!("未保存：{}", "Not saved: {}", error), true),
            }
        }
    }

    pub(super) fn autosave_app_shortcuts(&self) {
        let Some(window) = self.app_shortcuts_window() else {
            return;
        };
        let result = window.candidate().and_then(|shortcuts| {
            let mut config = self.ivars().config.borrow().clone();
            config.app_shortcuts = shortcuts;
            self.apply_config(config)
        });
        match result {
            Ok(()) => window.report(
                tr!(
                    "有效的快捷键已自动保存；未选择应用的行暂不启用。",
                    "Valid shortcuts saved automatically. Rows without an app stay inactive."
                ),
                false,
            ),
            Err(error) => window.report(&trf!("未保存：{}", "Not saved: {}", error), true),
        }
    }

    pub(super) fn apply_config(&self, candidate: Config) -> Result<(), String> {
        candidate.validate()?;
        let previous = self.ivars().config.borrow().clone();
        if candidate == previous {
            return Ok(());
        }
        let bindings_changed = candidate.shortcut != previous.shortcut
            || candidate.additional_search_shortcuts != previous.additional_search_shortcuts
            || candidate.switch_shortcut != previous.switch_shortcut
            || candidate.app_bindings()? != previous.app_bindings()?
            || candidate.quicklink_bindings()? != previous.quicklink_bindings()?
            || candidate.command_bindings()? != previous.command_bindings()?;
        let registration = if bindings_changed {
            Some(ShortcutTap::new(
                self.mtm(),
                &candidate,
                self.ivars().wake.get().unwrap().handle(),
            )?)
        } else {
            None
        };
        preference_store::save(
            &candidate,
            self.ivars()
                .config_store
                .get_or_init(NSUserDefaults::standardUserDefaults),
        )?;
        if let Some((tap, receiver)) = registration {
            self.ivars().shortcut_tap.replace(Some(tap));
            self.ivars().shortcut_rx.replace(Some(receiver));
            self.ivars().hotkey_error.replace(None);
        }
        if candidate.appearance != previous.appearance {
            preference_store::apply_appearance(&candidate, self.mtm());
        }
        if let Some(settings) = self.settings_window() {
            settings.set_app_shortcuts(&candidate.app_shortcuts);
            settings.set_command_shortcuts(&candidate.command_shortcuts);
            settings.set_alias_rules(&candidate.alias_rules);
            settings.set_snippets(&candidate.snippets);
            settings.set_quicklinks(&candidate.quicklinks);
            settings.fill_clipboard(&candidate.clipboard);
        }
        let language_changed = candidate.language != previous.language
            && preference_store::apply_language(candidate.language);
        if let Some(clipboard) = self.ivars().clipboard.borrow_mut().as_mut() {
            clipboard.configure(candidate.clipboard.clone());
        }
        input_source::set_policy(candidate.input_method, self.mtm());
        self.ivars().config.replace(candidate);
        self.configure_clipboard_timer();
        self.update_aliases(&self.ivars().windows.borrow());
        if language_changed {
            self.rebuild_localized_ui();
        }
        if bindings_changed {
            self.update_shortcut_labels();
        }
        self.filter();
        Ok(())
    }

    pub(super) fn show_app_shortcuts(&self) {
        self.cancel_routing();
        self.end_session();
        self.ivars().launch_receiver.replace(None);
        let window = self.ensure_app_shortcuts_window();
        NSApplication::sharedApplication(self.mtm()).activate();
        window.show(
            &self.ivars().config.borrow().app_shortcuts,
            self,
            self.mtm(),
        );
    }
}
