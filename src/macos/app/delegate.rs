use super::*;
use crate::macos::platform::preferences as preference_store;

define_class!(
    // SAFETY: All AppKit state is main-thread-only and lives until the application terminates.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = AppState]
    pub(super) struct Delegate;
    unsafe impl NSObjectProtocol for Delegate {}
    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidBecomeActive:))]
        fn became_active(&self, _: &NSNotification) {
            self.check_language();
            self.ensure_scrolling();
            if let Some(settings) = self.settings_window() { settings.update_login_status(); }
            self.finish_settings_focus(false);
        }
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_launch(&self, _: &NSNotification) {
            preference_store::apply_language(winlane::core::i18n::Language::System);
            match preference_store::load() {
                Ok(config) => { self.ivars().config.replace(config); }
                Err(error) => { self.ivars().hotkey_error.replace(Some(error)); }
            }
            crate::macos::platform::logging::init(&self.ivars().config.borrow().logging);
            preference_store::apply_language(self.ivars().config.borrow().language);
            input_source::set_rules(&self.ivars().config.borrow().input_rules, self.mtm());
            self.restore_recency(NSUserDefaults::standardUserDefaults());
            match preference_store::load_aliases() {
                Ok(aliases) => {
                    self.ivars().aliases.replace(aliases.with_rules(&[], &HashMap::new(), &self.ivars().config.borrow().alias_rules));
                    self.ivars().automatic_aliases.replace(aliases);
                    self.ivars().aliases_writable.set(true);
                }
                Err(error) => { self.ivars().alias_error.replace(Some(error)); }
            }
            preference_store::apply_appearance(&self.ivars().config.borrow(), self.mtm());
            match crate::macos::platform::updater::Updater::new(self.mtm(), self) {
                Ok(Some(updater)) => { let _ = self.ivars().updater.set(updater); }
                Ok(None) => {}
                Err(error) => {
                    crate::macos::platform::logging::record(crate::macos::platform::logging::Level::Warn, "updater", "init-failed", || error.clone());
                    self.ivars().updater_error.replace(Some(error));
                }
            }
            self.restore_app_input_history();
            self.build_ui();
            self.configure_app_input_rules();
            self.update_input_indicator();
            self.preload_projects();
            self.preload_files();
            self.observe_app_catalog();
            self.ivars().clipboard.replace(Some(crate::macos::platform::clipboard::ClipboardRuntime::new(self.ivars().config.borrow().clipboard.clone())));
            self.configure_clipboard_timer();
            self.register_hotkeys();
            self.ensure_scrolling();
            if !accessibility::is_trusted() { self.show(); } else { self.refresh(); }
        }
        #[unsafe(method(applicationWillTerminate:))]
        fn will_terminate(&self, _: &NSNotification) {
            self.ivars().auto_appclose.borrow().cancel();
            let _ = self.ivars().keep_awake.borrow_mut().apply(winlane::features::keep_awake::Choice::Stop);
            self.ivars().keep_awake_indicator.take();
            self.ivars().scroll_tap.take();
            self.ivars().catalog_watcher.take();
            self.save_recency();
            self.save_app_input_history();
            self.ivars().clipboard.take();
            crate::macos::platform::logging::record(crate::macos::platform::logging::Level::Info, "app", "stop", String::new);
            crate::macos::platform::logging::flush();
        }
        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn reopen(&self, _: &NSApplication, _: bool) -> bool { self.show(); true }
        #[unsafe(method(applicationDidChangeScreenParameters:))]
        fn screens_changed(&self, _: &NSNotification) {
            self.update_input_indicator();
            if !self.ivars().panels.borrow().is_empty() {
                self.sync_displays();
                self.render();
                if self.ivars().mode.get().is_some() && self.ivars().switch_timer.borrow().is_none() { self.present_panels(); }
            }
        }
    }
    unsafe impl NSWindowDelegate for Delegate {
        #[unsafe(method(windowDidBecomeKey:))]
        fn became_key(&self, notification: &NSNotification) {
            let Some(panel) = notification.object().and_then(|object| object.downcast::<SearchPanel>().ok()) else { return; };
            self.remember_panel_display(&panel);
            self.ivars().check_panel_focus.set(false);
            self.focus_search();
        }
        #[unsafe(method(windowShouldClose:))]
        fn should_close(&self, window: &NSWindow) -> bool {
            if window.downcast_ref::<SearchPanel>().is_some() {
                self.dismiss(); false
            } else {
                window.makeFirstResponder(None);
                true
            }
        }
        #[unsafe(method(windowWillClose:))]
        fn settings_closed(&self, notification: &NSNotification) {
            if let Some(window) = notification.object().and_then(|object| object.downcast::<NSWindow>().ok())
                && self.settings_window().is_some_and(|settings| settings.window == window)
            {
                self.cancel_settings_focus();
                self.ivars().settings_release_pending.set(true);
                self.ivars().wake.get().unwrap().signal();
            }
        }
        #[unsafe(method(windowDidResignKey:))]
        fn resigned(&self, notification: &NSNotification) {
            let Some(window) = notification.object().and_then(|object| object.downcast::<NSWindow>().ok()) else { return; };
            if window.downcast_ref::<SearchPanel>().is_none() {
                window.makeFirstResponder(None);
                self.ivars().wake.get().unwrap().signal();
                return;
            }
            self.ivars().check_panel_focus.set(true);
            self.ivars().wake.get().unwrap().signal();
        }
    }
    unsafe impl NSTabViewDelegate for Delegate {
        #[unsafe(method(tabView:didSelectTabViewItem:))]
        fn settings_tab_changed(&self, _: &NSTabView, _: Option<&NSTabViewItem>) {
            if let Some(settings) = self.settings_window() { settings.layout_selected_tab(); }
            self.ensure_settings_editor();
        }
    }
    unsafe impl NSControlTextEditingDelegate for Delegate {
        #[unsafe(method(controlTextDidChange:))]
        fn text_changed(&self, notification: &NSNotification) {
            if self.ivars().syncing_controls.get() { return; }
            if let Some(control) = notification.object().and_then(|object| object.downcast::<NSControl>().ok()) && self.update_quicklink_argument(&control) { return; }
            if let Some(input) = notification.object().and_then(|object| object.downcast::<NSSearchField>().ok()) {
                self.ivars().query.replace(input.stringValue().to_string());
                self.filter();
            }
        }
        #[unsafe(method(controlTextDidBeginEditing:))]
        fn text_began(&self, notification: &NSNotification) {
            if let Some(control) = notification.object().and_then(|object| object.downcast::<NSControl>().ok()) {
                let previous = self.ivars().quicklink_input.borrow().as_ref().map(|input| input.active);
                self.remember_quicklink_field(&control);
                let current = self.ivars().quicklink_input.borrow().as_ref().map(|input| input.active);
                if previous != current && !self.ivars().changing_input_source.get() {
                    self.prepare_search_field();
                    self.focus_search();
                }
            }
        }
        #[unsafe(method(control:textView:doCommandBySelector:))]
        fn text_command(&self, control: &NSControl, editor: &NSTextView, command: Sel) -> bool {
            if NSTextInputClient::hasMarkedText(editor) { false }
            else if self.editing_quicklink() { self.quicklink_text_command(control, command) }
            else if command == sel!(insertTab:)
                && (self.complete_selected_file() || self.begin_selected_quicklink()) { true }
            else if command == sel!(moveDown:) || command == sel!(insertTab:) {
                self.move_selection(1); true
            } else if command == sel!(moveUp:) || command == sel!(insertBacktab:) {
                self.move_selection(-1); true
            } else if command == sel!(insertNewline:) {
                self.activate_selected(); true
            } else if command == sel!(cancelOperation:) {
                self.cancel_search(); true
            } else { false }
        }
    }
    unsafe impl NSTextFieldDelegate for Delegate {}
    unsafe impl NSSearchFieldDelegate for Delegate {}
    unsafe impl NSMenuItemValidation for Delegate {
        #[unsafe(method(validateMenuItem:))]
        fn validate_menu_item(&self, item: &NSMenuItem) -> bool {
            let action = item.action();
            if action == Some(sel!(checkForUpdates:)) {
                self.ivars().updater.get().is_some_and(crate::macos::platform::updater::Updater::can_check)
                    || self.ivars().updater_error.borrow().is_some()
            } else if action == Some(sel!(toggleScope:)) {
                item.setState(if self.ivars().current_app_only.get() { NSControlStateValueOn } else { NSControlStateValueOff });
                self.any_panel_visible() && self.ivars().mode.get() == Some(PanelMode::Search) && !self.scoped_search()
            } else if [sel!(minimizeChosen:), sel!(hideChosen:), sel!(copyTitle:), sel!(quickSelect:)].into_iter().any(|sel| action == Some(sel)) {
                let visible = self.any_panel_visible();
                visible && if action == Some(sel!(quickSelect:)) {
                    (item.tag() as usize) < self.match_count()
                } else { self.selected_window().is_some() }
            } else { true }
        }
    }
    impl Delegate {
        #[unsafe(method(refreshSearchScope:))]
        fn refresh_search_scope_action(&self, timer: &NSTimer) {
            self.refresh_search_scope(timer);
        }
        #[unsafe(method(presentSwitch:))]
        fn present_switch(&self, timer: &NSTimer) {
            self.drain_shortcut_actions();
            let state = self.ivars();
            let current = state.switch_timer.borrow().as_ref().is_some_and(|pending| std::ptr::eq(&**pending, timer));
            if current && state.mode.get() == Some(PanelMode::Switch)
                && !state.switch_selection.borrow().as_ref().is_some_and(SwitchSelection::released)
            { self.present_panels(); }
        }
        #[unsafe(method(workspaceActivated:))]
        fn workspace_activated(&self, _: &NSNotification) {
            self.request_app_input_rules();
            self.track_frontmost();
        }
        #[unsafe(method(appInputSourceChanged:))]
        fn app_input_source_changed(&self, _: &NSNotification) { self.remember_app_input(); }
        #[unsafe(method(workspaceTerminated:))]
        fn workspace_terminated(&self, _: &NSNotification) { self.check_window_liveness(); }
        #[unsafe(method(warmPanelCache:))]
        fn warm_panel_cache(&self, _: &NSTimer) {
            if !self.warm_cache_step()
                && let Some(timer) = self.ivars().cache_warmup_timer.take() { timer.invalidate(); }
        }
        #[unsafe(method(inputSourceChanged:))]
        fn input_source_changed(&self, _: &NSNotification) {
            if !self.ivars().changing_input_source.get() {
                self.complete_input_start();
                self.remember_search_input();
            }
        }
        #[unsafe(method(indicatorSourceChanged:))]
        fn indicator_source_changed(&self, _: &NSNotification) { self.update_input_indicator(); }
        #[unsafe(method(indicatorSourcesChanged:))]
        fn indicator_sources_changed(&self, _: &NSNotification) {
            if let Some(settings) = self.settings_window() { settings.refresh_input_sources(); }
            self.update_input_indicator();
        }
        #[unsafe(method(indicatorSourceSelected:))]
        fn indicator_source_selected(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.settings_window() { settings.indicator_source_selected(); }
        }
        #[unsafe(method(resetIndicatorColor:))]
        fn reset_indicator_color(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.settings_window() { settings.reset_indicator_color(); }
            self.autosave_settings();
        }
        #[unsafe(method(finishSearchInputStart:))]
        fn finish_input_start(&self, timer: &NSTimer) {
            if self.ivars().input_start_timer.borrow().as_ref()
                .is_some_and(|pending| std::ptr::eq(&**pending, timer))
            { self.complete_input_start(); }
        }
        #[unsafe(method(showSettings:))]
        fn settings_action(&self, _: Option<&AnyObject>) {
            self.ivars().launch_receiver.replace(None);
            self.cancel_routing();
            self.end_session();
            let settings = self.ensure_settings_window();
            self.request_settings_focus(&settings);
            self.update_update_settings();
            self.report_shortcut_status();
        }
        #[unsafe(method(retrySettingsFocus:))]
        fn retry_settings_focus(&self, timer: &NSTimer) {
            let current = self.ivars().settings_focus_timer.borrow().as_ref()
                .is_some_and(|pending| std::ptr::eq(&**pending, timer));
            if current { self.finish_settings_focus(true); }
        }
        #[unsafe(method(checkForUpdates:))]
        fn check_for_updates(&self, _: Option<&AnyObject>) {
            self.prepare_update_ui();
            if let Some(updater) = self.ivars().updater.get() {
                updater.check();
            } else if let Some(error) = self.ivars().updater_error.borrow().as_ref() {
                let alert = NSAlert::new(self.mtm());
                alert.setMessageText(&NSString::from_str(tr!("无法检查更新", "Unable to Check for Updates")));
                alert.setInformativeText(&NSString::from_str(error));
                alert.runModal();
            }
        }
        #[unsafe(method(toggleAutomaticUpdates:))]
        fn toggle_automatic_updates(&self, sender: &NSButton) {
            if let Some(updater) = self.ivars().updater.get() {
                updater.set_automatic_checks(sender.state() == NSControlStateValueOn);
            }
            self.update_update_settings();
        }
        #[unsafe(method(standardUserDriverWillShowModalAlert))]
        fn update_will_show_alert(&self) { self.prepare_update_ui(); }
        #[unsafe(method(standardUserDriverWillHandleShowingUpdate:forUpdate:state:))]
        fn update_will_show(&self, showing: bool, _: &AnyObject, _: &AnyObject) {
            if showing { self.prepare_update_ui(); }
        }
        #[unsafe(method(selectInputRule:))]
        fn select_input_rule(&self, sender: &NSButton) {
            if let Some(settings) = self.settings_window() { settings.select_input_rule(sender.tag() as usize); }
        }
        #[unsafe(method(addInputRule:))]
        fn add_input_rule(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.settings_window() { settings.add_input_rule(); }
        }
        #[unsafe(method(removeInputRule:))]
        fn remove_input_rule(&self, sender: &NSButton) {
            if let Some(settings) = self.settings_window() { settings.remove_input_rule(sender.tag() as usize); }
            self.autosave_settings();
        }
        #[unsafe(method(chooseInputRuleApp:))]
        fn choose_input_rule_app(&self, sender: &NSButton) {
            if let Some(settings) = self.settings_window() { settings.choose_input_rule_app(sender.tag() as usize); }
        }
        #[unsafe(method(settingsChanged:))]
        fn settings_changed(&self, _: Option<&AnyObject>) { self.autosave_settings(); }
        #[unsafe(method(addSearchShortcut:))]
        fn add_search_shortcut(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.settings_window() { settings.add_search_shortcut(); }
        }
        #[unsafe(method(selectSettingsSection:))]
        fn select_settings_section(&self, sender: &NSButton) {
            if let Some(settings) = self.settings_window() { settings.select_tab(sender.tag()); }
        }
        #[unsafe(method(removeSearchShortcut:))]
        fn remove_search_shortcut(&self, sender: &NSButton) {
            if let Some(settings) = self.settings_window() {
                settings.remove_search_shortcut(sender.tag() as usize);
                self.autosave_settings();
            }
        }
        #[unsafe(method(resetSettings:))]
        fn reset_settings(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.settings_window() {
                if self.ivars().saving_settings.replace(true) { return; }
                let defaults = Config { snippets: self.ivars().config.borrow().snippets.clone(), quicklinks: self.ivars().config.borrow().quicklinks.clone(), ..Config::default() };
                let result = self.apply_config(defaults);
                self.ivars().saving_settings.set(false);
                match result {
                    Ok(()) => {
                        if let Some(settings) = self.settings_window() {
                            settings.fill(&self.ivars().config.borrow());
                            settings.report(tr!("默认设置已恢复并保存。登录启动状态保持不变。", "Defaults restored and saved. Launch at login is unchanged."), false);
                        }
                        if let Some(window) = self.app_shortcuts_window() {
                            window.fill(&self.ivars().config.borrow().app_shortcuts, self, self.mtm());
                        }
                        self.ivars().alias_rules_draft.take();
                        if let Some(window) = self.ivars().alias_rules_editor.borrow().clone() {
                            window.fill(&self.ivars().config.borrow().alias_rules, self, self.mtm());
                        }
                    }
                    Err(error) => settings.report(&trf!("未保存：{}", "Not saved: {}", error), true),
                }
            }
        }
        #[unsafe(method(changeBackgroundOpacity:))]
        fn change_background_opacity(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.settings_window() {
                settings.opacity_slider_changed();
                self.autosave_settings();
            }
        }
        #[unsafe(method(commitBackgroundOpacity:))]
        fn commit_background_opacity(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.settings_window() {
                match settings.opacity_input_changed() {
                    Ok(()) => self.autosave_settings(),
                    Err(error) => settings.report(&trf!("未保存：{}", "Not saved: {}", error), true),
                }
            }
        }
        #[unsafe(method(selectAliasRule:))]
        fn select_alias_rule(&self, sender: &NSButton) {
            self.ensure_alias_rules_editor().select(sender.tag() as usize);
        }
        #[unsafe(method(addAliasRule:))]
        fn add_alias_rule(&self, _: Option<&AnyObject>) {
            self.ensure_alias_rules_editor().add(self, self.mtm());
        }
        #[unsafe(method(removeAliasRule:))]
        fn remove_alias_rule(&self, sender: &NSButton) {
            self.ensure_alias_rules_editor().remove(sender.tag() as usize);
            self.autosave_alias_rules();
        }
        #[unsafe(method(chooseAliasApp:))]
        fn choose_alias_app(&self, sender: &NSButton) {
            self.ensure_alias_rules_editor().choose(sender.tag() as usize, self.mtm());
        }
        #[unsafe(method(aliasRuleTargetChanged:))]
        fn alias_rule_target_changed(&self, sender: &NSPopUpButton) {
            self.ensure_alias_rules_editor().update_target(sender.tag() as usize);
            self.autosave_alias_rules();
        }
        #[unsafe(method(aliasRulesChanged:))]
        fn alias_rules_changed(&self, _: Option<&AnyObject>) { self.autosave_alias_rules(); }
        #[unsafe(method(showAppShortcuts:))]
        fn app_shortcuts_action(&self, _: Option<&AnyObject>) {
            self.show_app_shortcuts();
        }
        #[unsafe(method(addAppShortcut:))]
        fn add_app_shortcut(&self, _: Option<&AnyObject>) {
            if let Some(window) = self.app_shortcuts_window() { window.add(self, self.mtm()); }
        }
        #[unsafe(method(removeAppShortcut:))]
        fn remove_app_shortcut(&self, sender: &NSButton) {
            if let Some(window) = self.app_shortcuts_window() {
                window.remove(sender.tag() as usize);
                self.autosave_app_shortcuts();
            }
        }
        #[unsafe(method(chooseShortcutApp:))]
        fn choose_shortcut_app(&self, sender: &NSButton) {
            if let Some(window) = self.app_shortcuts_window() { window.choose(sender.tag() as usize, self.mtm()); }
        }
        #[unsafe(method(appShortcutsChanged:))]
        fn app_shortcuts_changed(&self, _: Option<&AnyObject>) { self.autosave_app_shortcuts(); }
        #[unsafe(method(toggleLogin:))]
        fn toggle_login(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.settings_window() { settings.toggle_login(); }
        }
        #[unsafe(method(openLogs:))]
        fn open_logs(&self, _: Option<&AnyObject>) {
            if let Ok(path) = crate::macos::platform::logging::directory(&self.ivars().config.borrow().logging) {
                let _ = std::fs::create_dir_all(&path);
                NSWorkspace::sharedWorkspace().openURL(&NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy())));
            }
        }
        #[unsafe(method(manageLogin:))]
        fn manage_login(&self, _: Option<&AnyObject>) { preference_store::manage_login(); }
        #[unsafe(method(quicklinkArgumentChanged:))]
        fn quicklink_argument_changed(&self, control: &NSControl) { self.update_quicklink_argument(control); }
        #[unsafe(method(leaveScopedSearch:))]
        fn scope_back(&self, _: Option<&AnyObject>) { self.leave_scoped_search(); }
        #[unsafe(method(toggleScope:))]
        fn toggle_scope(&self, _: Option<&AnyObject>) {
            if self.scoped_search() { return; }
            self.ivars().current_app_only.set(!self.ivars().current_app_only.get());
            self.filter();
        }
        #[unsafe(method(minimizeChosen:))]
        fn minimize_chosen(&self, _: Option<&AnyObject>) { self.minimize_selected(); }
        #[unsafe(method(hideChosen:))]
        fn hide_chosen(&self, _: Option<&AnyObject>) { self.hide_selected(); }
        #[unsafe(method(copyTitle:))]
        fn copy_title(&self, _: Option<&AnyObject>) {
            if !self.any_panel_visible() { return; }
            if let Some(window) = self.selected_window() {
                let pasteboard = NSPasteboard::generalPasteboard();
                pasteboard.clearContents();
                // SAFETY: AppKit exports this immutable pasteboard type on all supported systems.
                if pasteboard.setString_forType(&NSString::from_str(&window.title), unsafe { NSPasteboardTypeString }) {
                    self.report_switch_error(tr!("已复制窗口标题。", "Window title copied."));
                } else { self.report_switch_error(tr!("无法写入剪贴板，请重试。", "Could not copy to the clipboard. Try again.")); }
            }
        }
        #[unsafe(method(quickSelect:))]
        fn quick_select(&self, sender: &NSMenuItem) {
            if !self.any_panel_visible() { return; }
            if (sender.tag() as usize) < self.match_count() {
                self.ivars().selected.set(sender.tag() as usize);
                self.render(); self.activate_selected();
            }
        }
        #[unsafe(method(showSearch:))]
        fn show_action(&self, _: Option<&AnyObject>) { self.show(); }
        #[unsafe(method(showSwitcher:))]
        fn switch_action(&self, _: Option<&AnyObject>) { self.open_switcher(); }
        #[unsafe(method(closeWindow:))]
        fn close_window(&self, _: Option<&AnyObject>) {
            if let Some(form) = self.ivars().snippet_arguments.borrow().as_ref() && form.window().isKeyWindow() { form.window().close(); return; }
            if let Some(settings) = self.settings_window() && settings.window.isKeyWindow() {
                settings.window.makeFirstResponder(None);
                settings.window.close();
            } else if self.any_panel_key() { self.dismiss(); }
        }
        #[unsafe(method(refreshFiles:))]
        fn refresh_files_timer(&self, timer: &NSTimer) { self.files_timer_fired(timer); }
        #[unsafe(method(clearRecentFiles:))]
        fn clear_recent_files_action(&self, _: Option<&AnyObject>) { self.clear_recent_files(); }
        #[unsafe(method(showFileActions:))]
        fn show_file_actions_button(&self, _: Option<&AnyObject>) { self.show_file_actions(); }
        #[unsafe(method(performFileAction:))]
        fn perform_file_menu_action(&self, sender: &NSMenuItem) { self.file_menu_action(sender); }
        #[unsafe(method(refreshWindows:))]
        fn refresh_action(&self, _: Option<&AnyObject>) {
            if self.searching_files() { self.refresh_files(); self.render(); return; }
            if self.searching_bluetooth() { self.refresh_bluetooth(); self.render(); return; }
            if self.searching_open_url() { self.refresh_open_url(); self.render(); return; }
            if self.searching_meeting() { self.refresh_meeting(); self.render(); return; }
            if self.searching_projects() { self.refresh_projects(true); self.render(); return; }
            if self.ivars().demo.get() { self.filter(); } else {
                self.invalidate_app_catalog();
                self.refresh();
            }
        }
        #[unsafe(method(pickWindow:))]
        fn pick(&self, sender: &NSButton) {
            if let Some(window) = sender.window() { self.remember_panel_display(&window); }
            self.ivars().selected.set(sender.tag() as usize);
            self.render();
            self.activate_selected();
        }
        #[unsafe(method(openPermissions:))]
        fn permissions(&self, _: Option<&AnyObject>) {
            accessibility::request_permission();
            if let Some(url) = NSURL::URLWithString(ns_string!("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")) {
                NSWorkspace::sharedWorkspace().openURL(&url);
            }
        }
        #[unsafe(method(openFeedback:))]
        fn feedback(&self, _: Option<&AnyObject>) {
            self.ivars().launch_receiver.replace(None);
            self.cancel_routing();
            self.end_session();
            if let Some(url) = NSURL::URLWithString(ns_string!("https://github.com/chenyukang/winlane/issues")) {
                NSWorkspace::sharedWorkspace().openURL(&url);
            }
        }
        #[unsafe(method(openHistoryPermissions:))]
        fn history_permissions(&self, _: Option<&AnyObject>) {
            if !self.searching_open_url() || !self.ivars().open_url_history.borrow().access_denied {
                return;
            }
            self.cancel_routing();
            self.end_session();
            if let Some(url) = NSURL::URLWithString(ns_string!("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")) {
                NSWorkspace::sharedWorkspace().openURL(&url);
            }
        }
        #[unsafe(method(toggleDemo:))]
        fn toggle_demo(&self, _: Option<&AnyObject>) {
            let state = self.ivars();
            state.demo.set(!state.demo.get());
            state.query.borrow_mut().clear();
            if state.demo.get() {
                state.windows.replace(demo_windows());
                state.loading.set(false);
                self.filter();
            } else { state.windows.borrow_mut().clear(); self.refresh(); }
        }
        #[unsafe(method(pollClipboard:))]
        fn poll_clipboard(&self, _: Option<&AnyObject>) {
            let changed = self.ivars().clipboard.borrow_mut().as_mut().is_some_and(|clipboard| clipboard.poll_clipboard());
            if changed && self.searching_clipboard() { self.filter_preserving(self.selected_result()); }
        }
        #[unsafe(method(clipboardActions:))]
        fn clipboard_action(&self, sender: &NSPopUpButton) {
            let action = sender.indexOfSelectedItem(); sender.selectItemAtIndex(0);
            match action { 1 => self.use_clipboard(false), 2 => self.delete_clipboard_entry(), 3 => self.toggle_clipboard_recording(), 4 => self.confirm_clear_clipboard(), _ => {} }
        }
        #[unsafe(method(retryScrolling:))]
        fn retry_scrolling(&self, _: Option<&AnyObject>) {
            self.autosave_settings();
            self.ensure_scrolling();
        }
        #[unsafe(method(addAppCloseRule:))]
        fn add_appclose_rule(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.settings_window() { settings.add_appclose_rule(); }
        }
        #[unsafe(method(toggleAutoAppClose:))]
        fn toggle_auto_appclose(&self, _: Option<&AnyObject>) {
            let Some(settings) = self.settings_window() else { return; };
            if settings.appclose_enabled() { self.autosave_settings(); return; }
            self.ivars().auto_appclose.borrow().cancel();
            // The off switch must work even while another field contains an invalid draft.
            let mut config = self.ivars().config.borrow().clone();
            config.auto_appclose.enabled = false;
            match self.apply_config(config) {
                Ok(()) => settings.report(tr!("已关闭，保留现有规则。", "Off. Your rules are kept."), false),
                Err(error) => settings.report(&error, true),
            }
        }
        #[unsafe(method(removeAppCloseRule:))]
        fn remove_appclose_rule(&self, sender: &NSButton) {
            if let Some(settings) = self.settings_window() { settings.remove_appclose_rule(sender.tag() as usize); }
            self.autosave_settings();
        }
        #[unsafe(method(chooseAppCloseApp:))]
        fn choose_appclose_app(&self, sender: &NSButton) {
            if let Some(settings) = self.settings_window() { settings.choose_appclose_app(sender.tag() as usize); }
        }
        #[unsafe(method(clearClipboardHistory:))]
        fn clear_clipboard_action(&self, _: Option<&AnyObject>) { self.confirm_clear_clipboard(); }
        #[unsafe(method(poll:))]
        fn poll(&self, _: Option<&AnyObject>) {
            self.update_app_input_rules();
            self.release_closed_settings();
            self.update_scrolling_status();
            let transient_ui_open = self.ivars().files.borrow().menu_open
                || (self.searching_bluetooth() && self.ivars().bluetooth_permission.borrow().is_some());
            if self.ivars().check_panel_focus.replace(false)
                && self.ivars().mode.get().is_some()
                && !self.ivars().changing_displays.get()
                && !self.any_panel_key()
                && !transient_ui_open
            { self.end_session(); }
            self.check_shortcuts();
            self.poll_window_liveness();
            self.drain_shortcut_actions();
            self.poll_focus();
            self.poll_auto_appclose();
            self.poll_app_launch();
            self.poll_project_open();
            self.poll_app_catalog_changes();
            self.poll_app_catalog();
            self.poll_projects();
            self.poll_open_url();
            self.poll_meeting();
            self.poll_files();
            self.poll_bluetooth();
            let awake_expired = self.ivars().keep_awake.borrow_mut().expire(std::time::SystemTime::now());
            let awake_changed = self.update_keep_awake_indicator(false);
            if (awake_expired || awake_changed) && self.searching_keep_awake() {
                self.render();
            }
            let clipboard_changed = self.ivars().clipboard.borrow_mut().as_mut().is_some_and(|clipboard| clipboard.poll_storage());
            if clipboard_changed && self.searching_clipboard() { self.filter_preserving(self.selected_result()); }
            let result = self.ivars().receiver.borrow().as_ref().map(|rx| rx.try_recv());
            if let Some(Ok(mut snapshot)) = result {
                self.ivars().receiver.replace(None);
                self.ivars().loading.set(false);
                if !self.ivars().demo.get() {
                    if !accessibility::is_trusted() {
                        snapshot.windows.clear();
                        snapshot.server_ids.clear();
                    }
                    self.install_window_snapshot(snapshot);
                }
            } else if matches!(result, Some(Err(TryRecvError::Disconnected))) {
                self.ivars().receiver.replace(None);
                self.ivars().loading.set(false);
                self.render();
                if self.ivars().mode.get().is_some() { self.switch_to_search(); }
                self.report_switch_error(tr!("读取窗口失败，请按 ⌘R 重试。", "Could not read windows. Press ⌘R to retry."));
            }
        }
        #[unsafe(method(quitApp:))]
        fn quit(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.settings_window() {
                settings.window.makeFirstResponder(None);
            }
            self.end_session();
            self.ivars().clipboard.take();
            NSApplication::sharedApplication(self.mtm()).terminate(None);
        }
    }
);

impl Delegate {
    pub(super) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(AppState::default());
        // SAFETY: NSObject's initializer has this exact signature.
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        let weak = Weak::new(&*this);
        let wake = MainWake::new(mtm, move || {
            if let Some(delegate) = weak.load() {
                delegate.poll(sel!(poll:), None);
            }
        });
        assert!(this.ivars().wake.set(wake).is_ok());
        this
    }

    pub(super) fn build_ui(&self) {
        let mtm = self.mtm();
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        self.build_menus();
        // SAFETY: The delegate lives for the app lifetime; AppKit delivers this notification on the main thread.
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                self,
                sel!(inputSourceChanged:),
                Some(NSTextInputContextKeyboardSelectionDidChangeNotification),
                None,
            );
        }
        // SAFETY: Workspace delivers application activation notifications on the main thread.
        unsafe {
            NSWorkspace::sharedWorkspace()
                .notificationCenter()
                .addObserver_selector_name_object(
                    self,
                    sel!(workspaceActivated:),
                    Some(NSWorkspaceDidActivateApplicationNotification),
                    None,
                );
            NSWorkspace::sharedWorkspace()
                .notificationCenter()
                .addObserver_selector_name_object(
                    self,
                    sel!(workspaceTerminated:),
                    Some(NSWorkspaceDidTerminateApplicationNotification),
                    None,
                );
        }
        self.observe_input_indicator();
        self.track_frontmost();
        // SAFETY: The application retains this delegate for the entire run loop; poll: has NSTimer signature.
        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                1.0,
                self,
                sel!(poll:),
                None,
                true,
            )
        };
        timer.setTolerance(0.1);
        // SAFETY: Permission and event-tap health checks also run during menu tracking.
        // Keyboard and scan results wake the run loop independently of this timer.
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
        self.ivars().timer.set(timer).unwrap();
    }
}

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../tests/native/app/mod.rs"]
pub(crate) mod tests;
