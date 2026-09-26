use super::*;

impl Delegate {
    pub(super) fn activate_selected(&self) {
        if self.searching_emoji() {
            self.use_emoji();
            return;
        }
        if self.searching_files() {
            self.open_selected_file(false);
            return;
        }
        if self.searching_keep_awake() {
            self.apply_keep_awake();
            return;
        }
        if self.searching_bluetooth() {
            self.toggle_selected_bluetooth();
            return;
        }
        if let Some(scope) = self
            .selected_command()
            .and_then(super::commands::command_scope)
        {
            self.enter_scoped_search(scope);
            return;
        }
        if self.searching_open_url() {
            self.submit_open_url(false);
            return;
        }
        if self.searching_meeting() {
            self.submit_meeting();
            return;
        }
        if self.searching_git_branch() {
            self.submit_git_branch();
            return;
        }
        if self.searching_projects() {
            self.open_selected_project();
            return;
        }
        if self.editing_quicklink() {
            self.submit_quicklink_input();
            return;
        }
        if self.searching_quicklinks() && self.match_count() == 0 {
            return;
        }
        if self.searching_clipboard() {
            self.use_clipboard(true);
            return;
        }
        if self.searching_snippets() && self.match_count() == 0 {
            return;
        }
        if let Some(link) = self.selected_quicklink() {
            self.use_quicklink(&link);
            return;
        }
        if let Some(tap) = self.ivars().shortcut_tap.borrow().as_ref() {
            tap.finish(self.ivars().session.get());
        }
        self.ivars().switch_selection.replace(None);
        if let Some(command) = self.selected_command() {
            self.execute_command(command);
            return;
        }
        if let Some(snippet) = self.selected_snippet() {
            self.use_snippet(&snippet);
            return;
        }
        if let Some(application) = self.selected_application() {
            self.launch_application(&application, LaunchOrigin::Search);
            return;
        }
        let Some(window) = self.selected_window() else {
            self.selection_failed(tr!(
                "没有匹配的窗口，请重新搜索。",
                "No matching window. Search again."
            ));
            return;
        };
        if self.ivars().demo.get() {
            self.selection_failed(&trf!(
                "演示选择：{} · {}（未切换真实窗口）",
                "Demo selection: {} · {} (real windows unchanged)",
                window.app,
                window.title
            ));
            return;
        }
        let Some(target_app) =
            NSRunningApplication::runningApplicationWithProcessIdentifier(window.pid)
        else {
            self.selection_failed(tr!(
                "应用已退出，请按 ⌘R 更新窗口列表。",
                "The app has quit. Press ⌘R to refresh windows."
            ));
            return;
        };
        target_app.unhide();
        if window.minimized {
            // A minimized window lives on no Space until restored. Activate
            // the app through LaunchServices first so macOS switches Spaces
            // from a full-screen app, then restore and focus the chosen
            // window on the app's own Space once activation completes.
            if let Err(error) = crate::macos::platform::applications::activate_and_raise(
                &target_app,
                window.id,
                self.ivars().wake.get().unwrap().handle(),
            ) {
                // Apps without a bundle URL cannot go through LaunchServices.
                // Restore directly and fall back to plain activation.
                if accessibility::raise_window(window.pid, window.id).is_err()
                    || !self.activate_app(&target_app)
                {
                    self.selection_failed(&error);
                    return;
                }
            }
        } else {
            if let Err(error) = accessibility::raise_window(window.pid, window.id) {
                // A single-window app can still be switched to when AX cannot
                // target its window. Do not use this fallback
                // for multi-window apps, where it could select the wrong one.
                let only_window = self
                    .ivars()
                    .windows
                    .borrow()
                    .iter()
                    .filter(|candidate| candidate.pid == window.pid)
                    .count()
                    == 1;
                if !only_window || !self.activate_app(&target_app) {
                    self.selection_failed(&error);
                    return;
                }
            } else if !self.activate_app(&target_app) {
                self.selection_failed(tr!(
                    "系统未接受切换请求，请重试或检查辅助功能权限。",
                    "macOS did not accept the switch. Try again or check Accessibility access."
                ));
                return;
            }
        }
        self.remember_window(window.id);
        let query = self.ivars().query.borrow().trim().to_lowercase();
        if !query.is_empty() {
            let mut preferences = self.ivars().preferences.borrow_mut();
            if preferences.len() > 128 {
                preferences.clear();
            }
            preferences.insert(query, window.id);
        }
        self.end_session();
    }

    pub(super) fn selection_failed(&self, text: &str) {
        let session = self.ivars().session.get();
        if let Some(tap) = self.ivars().shortcut_tap.borrow().as_ref() {
            tap.resume_search(session);
        }
        self.display_search(session);
        self.report_switch_error(text);
    }

    pub(super) fn report_switch_error(&self, text: &str) {
        for ui in self.panels() {
            ui.footer.setHidden(false);
            ui.footer.setStringValue(&NSString::from_str(text));
            ui.footer.setToolTip(Some(&NSString::from_str(text)));
        }
    }

    pub(super) fn minimize_selected(&self) {
        if !self.any_panel_visible() {
            return;
        }
        if self.ivars().mode.get() == Some(PanelMode::Switch) {
            self.switch_to_search();
        }
        let Some(window) = self.selected_window() else {
            return;
        };
        let minimized = !window.minimized;
        if !self.ivars().demo.get()
            && let Err(error) = accessibility::set_minimized(window.pid, window.id, minimized)
        {
            self.report_switch_error(&error);
            return;
        }
        self.ivars().receiver.replace(None);
        self.ivars().loading.set(false);
        if let Some(item) = self
            .ivars()
            .windows
            .borrow_mut()
            .iter_mut()
            .find(|item| item.id == window.id)
        {
            item.minimized = minimized;
        }
        self.filter_preserving(Some(SelectedResult::Window(window.id)));
        self.report_switch_error(if self.ivars().demo.get() {
            tr!(
                "演示：已更新示例窗口的最小化状态，未操作真实窗口。",
                "Demo: minimized state updated; real windows unchanged."
            )
        } else if minimized {
            tr!("已最小化窗口。", "Window minimized.")
        } else {
            tr!("已恢复窗口。", "Window restored.")
        });
    }

    pub(super) fn hide_selected(&self) {
        if !self.any_panel_visible() {
            return;
        }
        let Some(window) = self.selected_window() else {
            return;
        };
        if self.ivars().demo.get() {
            self.report_switch_error(&trf!(
                "演示：隐藏 {}（未操作真实应用）。",
                "Demo: hide {} (real apps unchanged).",
                window.app
            ));
            return;
        }
        match NSRunningApplication::runningApplicationWithProcessIdentifier(window.pid) {
            Some(app) if app.hide() => self.report_switch_error(tr!(
                "已隐藏应用。选择它的窗口后按回车可重新打开。",
                "App hidden. Select its window and press Enter to show it again."
            )),
            _ => self.report_switch_error(tr!(
                "无法隐藏应用，它可能已经退出。",
                "Could not hide the app. It may have quit."
            )),
        }
    }
}
