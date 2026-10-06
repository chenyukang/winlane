use super::*;

impl Delegate {
    /// A row can outlive its window between a scan and a selection, before the
    /// background inventory check catches up. Confirming that here lets the
    /// stale row be dropped and explained, instead of failing with a message
    /// that asks for a manual refresh.
    pub(super) fn discard_missing_window(&self, window: &WindowInfo) -> bool {
        // The WindowServer decides, not accessibility: an app may be slow to
        // answer, and a rebuilt accessibility object changes a live window's
        // identity. A window without a recorded surface cannot be confirmed
        // closed, so its row is kept and the failure is reported instead.
        let Some(server_id) = self
            .ivars()
            .window_server_ids
            .borrow()
            .get(&window.id)
            .copied()
        else {
            return false;
        };
        let Some(inventory) = crate::macos::platform::window_server::Inventory::try_read() else {
            return false;
        };
        if inventory.contains(server_id) {
            return false;
        }
        crate::macos::platform::logging::record(
            crate::macos::platform::logging::Level::Debug,
            "switch",
            "stale-window-dropped",
            || format!("id={}", window.id),
        );
        self.remove_closed_windows(&[window.id]);
        self.selection_failed(tr!(
            "该窗口已关闭，已从列表中移除。",
            "That window has closed and has been removed from the list."
        ));
        true
    }

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
        // The row can outlive the process it was built from: the app may have
        // quit, or restarted with a new process identifier. Both are the same
        // app, and picking its window means "show me that app", so find it
        // again, and start it when nothing is running.
        let target_app = NSRunningApplication::runningApplicationWithProcessIdentifier(window.pid)
            .or_else(|| self.running_application_for_window(&window));
        let Some(target_app) = target_app else {
            if let Some(application) = self.application_for_window(&window) {
                self.launch_application(&application, LaunchOrigin::Search);
            } else {
                self.selection_failed(tr!(
                    "应用已退出并已卸载，请按 ⌘R 更新窗口列表。",
                    "The app has quit and is no longer installed. Press ⌘R to refresh windows."
                ));
            }
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
                if self.discard_missing_window(&window) {
                    return;
                }
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
                if self.discard_missing_window(&window) {
                    return;
                }
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

    /// The bundle identifier the last scan recorded for a window's app. The
    /// window list and these identities come from the same scan, so a row still
    /// knows which app it belonged to after that app quits.
    pub(super) fn window_bundle_id(&self, window: &WindowInfo) -> Option<String> {
        self.ivars()
            .identities
            .borrow()
            .get(&window.pid)
            .map(|identity| identity.id.trim().to_string())
            .filter(|id| !id.is_empty())
    }

    /// A running instance of the window's app, for when the process identifier
    /// the row was built from is gone but the app is running under a new one.
    pub(super) fn running_application_for_window(
        &self,
        window: &WindowInfo,
    ) -> Option<Retained<NSRunningApplication>> {
        let bundle_id = self.window_bundle_id(window)?;
        NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(
            &bundle_id,
        ))
        .iter()
        .find(|app| !app.isTerminated())
    }

    /// The installed app a window row belongs to, so it can be started again.
    pub(super) fn application_for_window(&self, window: &WindowInfo) -> Option<ApplicationTarget> {
        let bundle_id = self.window_bundle_id(window)?;
        let url = NSWorkspace::sharedWorkspace()
            .URLForApplicationWithBundleIdentifier(&NSString::from_str(&bundle_id))?;
        crate::macos::platform::applications::target_at_url(&url).ok()
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
