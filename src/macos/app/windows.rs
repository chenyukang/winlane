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
        if self.use_mode_selection() {
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
        self.activate_window(&window);
    }

    /// The modes where Enter belongs to a page of their own — emoji, files,
    /// keep-awake, Bluetooth, a scoped search, the clipboard — instead of the
    /// window list. Returns whether the key was handled here.
    fn use_mode_selection(&self) -> bool {
        if self.searching_emoji() {
            self.use_emoji();
            return true;
        }
        if self.searching_files() {
            self.open_selected_file(false);
            return true;
        }
        if self.searching_keep_awake() {
            self.apply_keep_awake();
            return true;
        }
        if self.searching_bluetooth() {
            self.toggle_selected_bluetooth();
            return true;
        }
        if let Some(scope) = self
            .selected_command()
            .and_then(super::commands::command_scope)
        {
            self.enter_scoped_search(scope);
            return true;
        }
        if self.searching_open_url() {
            self.submit_open_url(false);
            return true;
        }
        if self.searching_meeting() {
            self.submit_meeting();
            return true;
        }
        if self.searching_git_branch() {
            self.submit_git_branch();
            return true;
        }
        if self.searching_projects() {
            self.open_selected_project();
            return true;
        }
        if self.editing_quicklink() {
            self.submit_quicklink_input();
            return true;
        }
        if self.searching_quicklinks() && self.match_count() == 0 {
            return true;
        }
        if self.searching_clipboard() {
            self.use_clipboard(true);
            return true;
        }
        if self.searching_snippets() && self.match_count() == 0 {
            return true;
        }
        if let Some(link) = self.selected_quicklink() {
            self.use_quicklink(&link);
            return true;
        }
        false
    }

    /// Switch to a window. The row can outlive the process it was built from,
    /// so the app is found again, and started when nothing is running.
    fn activate_window(&self, window: &WindowInfo) {
        let started = Instant::now();
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
            .or_else(|| self.running_application_for_window(window));
        let Some(target_app) = target_app else {
            if let Some(application) = self.application_for_window(window) {
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
                if self.discard_missing_window(window) {
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
                if self.discard_missing_window(window) {
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
                if !only_window {
                    self.selection_failed(&error);
                    return;
                }
                // A bare activation leaves the window behind when its
                // accessibility handle is unusable — the app comes forward with
                // no window, which looks like the desktop. Let LaunchServices
                // activate the app and raise the window once activation
                // completes, the same path minimized windows use.
                crate::macos::platform::logging::record(
                    crate::macos::platform::logging::Level::Debug,
                    "switch",
                    "raise-failed",
                    || format!("app_pid={} window={}", window.pid, window.id),
                );
                if let Err(fallback) = crate::macos::platform::applications::activate_and_raise(
                    &target_app,
                    window.id,
                    self.ivars().wake.get().unwrap().handle(),
                ) {
                    if self.discard_missing_window(window) {
                        return;
                    }
                    // Apps without a bundle URL cannot go through LaunchServices.
                    if !self.activate_app(&target_app) {
                        self.selection_failed(&fallback);
                        return;
                    }
                }
            } else if !self.activate_app(&target_app) {
                self.selection_failed(tr!(
                    "系统未接受切换请求，请重试或检查辅助功能权限。",
                    "macOS did not accept the switch. Try again or check Accessibility access."
                ));
                return;
            }
        }
        // Chromium-based apps can hand focus to an internal window right after
        // activation, which leaves no visible window in front and looks like the
        // desktop. Check again shortly afterwards and put the chosen window back,
        // but never undo a switch the user made in the meantime.
        let (pid, id) = (window.pid, window.id);
        let server_id = self.ivars().window_server_ids.borrow().get(&id).copied();
        std::thread::spawn(move || retain_window_focus(pid, id, server_id));
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
        crate::macos::platform::recency_trace::record_slow("activate", started, || {
            format!("app_pid={} window={}", window.pid, window.id)
        });
    }

    /// Enter: save the name while renaming, otherwise switch to the selection.
    pub(super) fn accept_selection(&self) {
        crate::macos::platform::logging::record(
            crate::macos::platform::logging::Level::Info,
            "labels",
            "accept",
            || format!("renaming={:?}", self.ivars().renaming.get()),
        );
        if self.ivars().renaming.get().is_some() {
            let name = self.ivars().query.borrow().clone();
            self.rename_selected_window(&name);
        } else {
            self.activate_selected();
        }
    }

    /// Whether the row that is selected now is the one being renamed, so a stale
    /// mode cannot rename a window the user has since moved away from.
    pub(super) fn renaming_selected_window(&self) -> bool {
        let Some(id) = self.ivars().renaming.get() else {
            return false;
        };
        self.selected_window().is_some_and(|window| window.id == id)
    }

    /// Start renaming the selected window: the field holds its current name, so
    /// Enter can keep or replace it and Escape can drop it.
    pub(super) fn begin_rename_selected(&self) {
        let Some(window) = self.selected_window() else {
            self.selection_failed(tr!(
                "先选中一个窗口，再按 ⌃⏎ 重命名。",
                "Select a window first, then press Control+Enter to name it."
            ));
            return;
        };
        let current = self
            .labelled_window(&window)
            .map_or_else(String::new, |named| named.title);
        crate::macos::platform::logging::record(
            crate::macos::platform::logging::Level::Info,
            "labels",
            "begin",
            || format!("window={} prefilled={}", window.id, current.chars().count()),
        );
        self.ivars().renaming.set(Some(window.id));
        self.ivars().query.replace(current);
        self.filter();
        // Keep the row being named selected, so the panel shows what will change.
        if let Some(position) = {
            let windows = self.ivars().windows.borrow();
            let matches = self.ivars().matches.borrow();
            matches
                .iter()
                .position(|&index| windows.get(index).is_some_and(|item| item.id == window.id))
        } {
            self.ivars().selected.set(position);
            self.render();
        }
    }

    pub(super) fn cancel_rename(&self) {
        self.ivars().renaming.set(None);
        self.ivars().query.replace(String::new());
        self.filter();
    }

    /// Name the selected window. An empty name puts its real title back.
    pub(super) fn rename_selected_window(&self, name: &str) {
        let window = self
            .ivars()
            .renaming
            .get()
            .and_then(|id| {
                self.ivars()
                    .windows
                    .borrow()
                    .iter()
                    .find(|window| window.id == id)
                    .cloned()
            })
            .or_else(|| self.selected_window());
        let Some(window) = window else {
            self.selection_failed(tr!(
                "先选中一个窗口，再按 ⌥⏎ 重命名。",
                "Select a window first, then press Option+Return to name it."
            ));
            return;
        };
        let Some(bundle_id) = self
            .ivars()
            .identities
            .borrow()
            .get(&window.pid)
            .map(|identity| identity.id.clone())
        else {
            self.selection_failed(tr!(
                "这个窗口所属的应用还不明确，请先刷新窗口列表。",
                "Winlane does not know this window's app yet. Refresh the window list first."
            ));
            return;
        };
        let server_id = self
            .ivars()
            .window_server_ids
            .borrow()
            .get(&window.id)
            .copied();
        let identity = winlane::features::window_labels::WindowIdentity {
            bundle_id: &bundle_id,
            title: &window.title,
            server_id,
        };
        // "名字 #训练" sets both the name and the group; "#训练" only groups.
        let (name, tag) = winlane::features::window_labels::split_tag(name);
        let mut labels = self.ivars().window_labels.borrow_mut();
        let result = winlane::features::window_labels::set_label(&mut labels, &identity, &name)
            .and_then(|()| match tag.as_deref() {
                Some(tag) => winlane::features::window_labels::set_tag(&mut labels, &identity, tag),
                None => Ok(()),
            });
        match result {
            Ok(()) => {
                drop(labels);
                crate::macos::platform::logging::record(
                    crate::macos::platform::logging::Level::Info,
                    "labels",
                    "saved",
                    || {
                        format!(
                            "window={} length={} tag={:?}",
                            window.id,
                            name.chars().count(),
                            tag
                        )
                    },
                );
                self.ivars().renaming.set(None);
                self.ivars().query.replace(String::new());
                self.filter();
            }
            Err(error) => {
                drop(labels);
                self.selection_failed(&error);
            }
        }
    }

    /// The window as the row should show it: the name the user gave it replaces
    /// the title, while the app name and alias stay as they are.
    pub(super) fn labelled_window(&self, window: &WindowInfo) -> Option<WindowInfo> {
        let labels = self.ivars().window_labels.borrow();
        if labels.is_empty() {
            return None;
        }
        let bundle_id = self
            .ivars()
            .identities
            .borrow()
            .get(&window.pid)?
            .id
            .clone();
        let server_id = self
            .ivars()
            .window_server_ids
            .borrow()
            .get(&window.id)
            .copied();
        let label = winlane::features::window_labels::label_for(
            &labels,
            &winlane::features::window_labels::WindowIdentity {
                bundle_id: &bundle_id,
                title: &window.title,
                server_id,
            },
        )?;
        let title = match (label.label.trim(), label.tag.as_deref()) {
            ("", Some(tag)) => format!("· {tag}"),
            ("", None) => String::new(),
            (name, None) => name.to_owned(),
            (name, Some(tag)) => format!("{name} · {tag}"),
        };
        Some(WindowInfo {
            title,
            ..window.clone()
        })
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

/// Re-raise a window whose app moved focus to one of its internal windows.
///
/// Runs on a background thread and stops as soon as the app is no longer in
/// front, so a quick follow-up switch is never undone.
#[allow(deprecated)]
fn retain_window_focus(pid: i32, id: u64, server_id: Option<u32>) {
    for (attempt, pause) in [(0u8, 80u64), (1, 120)] {
        std::thread::sleep(std::time::Duration::from_millis(pause));
        // Never undo a switch the user made in the meantime.
        if NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|app| app.processIdentifier())
            != Some(pid)
        {
            return;
        }
        let focused = accessibility::focused_window_id(pid) == Some(id);
        // Focus says nothing about whether anything is drawn: the window can be
        // on another desktop or ordered out while it still holds focus.
        let onscreen = server_id.is_none_or(|server_id| {
            crate::macos::platform::window_server::Inventory::try_read_onscreen()
                .is_some_and(|inventory| inventory.contains(server_id))
        });
        if focused && onscreen {
            return;
        }
        // Ask for the app's windows the way a Dock click does, then raise this
        // one: that both shows an ordered-out window and moves focus back.
        if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
            app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
        }
        let _ = accessibility::raise_window(pid, id);
        crate::macos::platform::logging::record(
            crate::macos::platform::logging::Level::Debug,
            "switch",
            if focused {
                "focus-invisible"
            } else {
                "focus-corrected"
            },
            || format!("app_pid={pid} window={id} attempt={attempt}"),
        );
    }
    if accessibility::focused_window_id(pid) != Some(id) {
        crate::macos::platform::logging::record(
            crate::macos::platform::logging::Level::Debug,
            "switch",
            "focus-lost",
            || format!("app_pid={pid} window={id}"),
        );
    }
}
