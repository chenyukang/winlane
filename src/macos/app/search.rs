use super::*;

impl Delegate {
    pub(super) fn filter(&self) {
        self.filter_preserving(None);
    }

    pub(super) fn filter_preserving(&self, selected_id: Option<SelectedResult>) {
        if self.searching_emoji() {
            self.filter_emoji(selected_id);
            return;
        }
        self.ivars().emoji_matches.borrow_mut().clear();
        if self.searching_files() {
            self.filter_files(selected_id);
            return;
        }
        self.cancel_files_search();
        if self.searching_keep_awake() {
            self.filter_keep_awake(selected_id);
            return;
        }
        self.ivars().keep_awake_matches.borrow_mut().clear();
        if self.searching_bluetooth() {
            self.filter_bluetooth(selected_id);
            return;
        }
        self.ivars().bluetooth_matches.borrow_mut().clear();
        if self.searching_open_url() {
            self.filter_open_url(selected_id);
            return;
        }
        self.clear_open_url_matches();
        if self.searching_projects() {
            self.filter_projects(selected_id);
            return;
        }
        self.ivars().project_matches.borrow_mut().clear();
        if self.editing_quicklink() {
            self.filter_quicklink_input();
            return;
        }
        if self.searching_quicklinks() {
            self.filter_quicklinks(selected_id);
            return;
        }
        if self.searching_clipboard() {
            self.filter_clipboard(selected_id);
            return;
        }
        if self.searching_snippets() {
            self.filter_snippets(selected_id);
            return;
        }
        self.ensure_app_catalog();
        let query = self.ivars().query.borrow().clone();
        let preferred = self
            .ivars()
            .preferences
            .borrow()
            .get(&query.trim().to_lowercase())
            .copied();
        let windows = self.ivars().windows.borrow();
        let scope_pid = if self.ivars().current_app_only.get() {
            Some(if self.ivars().demo.get() {
                -1
            } else {
                self.ivars().previous_pid.get()
            })
        } else {
            None
        };
        let aliases = self.ivars().aliases.borrow();
        let is_alias = aliases.is_alias(&query);
        let matched = visible_matches(
            &windows,
            if is_alias { "" } else { &query },
            preferred,
            &self.ivars().config.borrow(),
            scope_pid,
            &self.ivars().recency.borrow(),
            self.ivars().previous_pid.get(),
        );
        let alias_matches = if self.ivars().mode.get() == Some(PanelMode::Search) {
            aliases
                .search_order(&query, &matched, &windows)
                .map(|mut results| {
                    let included: HashSet<_> = results.iter().copied().collect();
                    let text_matches = visible_matches(
                        &windows,
                        &query,
                        preferred,
                        &self.ivars().config.borrow(),
                        scope_pid,
                        &self.ivars().recency.borrow(),
                        self.ivars().previous_pid.get(),
                    );
                    results.extend(
                        text_matches
                            .into_iter()
                            .filter(|index| !included.contains(index)),
                    );
                    results
                })
        } else {
            aliases.filter_order(&query, &matched, &windows)
        };
        let matched = alias_matches.unwrap_or(matched);
        crate::macos::platform::recency_trace::record("order", || {
            format!(
                "mode={:?} filtered={} origin={}/{:?} windows={:?} recent={:?}",
                self.ivars().mode.get(),
                !query.is_empty(),
                self.ivars().previous_pid.get(),
                self.ivars().previous_window.get(),
                matched.iter().map(|&i| windows[i].id).collect::<Vec<_>>(),
                self.ivars().recency.borrow(),
            )
        });
        let command_matches = if self.ivars().mode.get() == Some(PanelMode::Search)
            && !self.ivars().demo.get()
            && scope_pid.is_none()
        {
            matching_commands(&query)
        } else {
            Vec::new()
        };
        let quicklink_matches = if self.ivars().mode.get() == Some(PanelMode::Search)
            && !self.ivars().demo.get()
            && scope_pid.is_none()
            && !query.trim().is_empty()
        {
            let config = self.ivars().config.borrow();
            winlane::features::quicklinks::matching(&config.quicklinks, &query)
                .into_iter()
                .map(|i| config.quicklinks[i].clone())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let snippet_matches: Vec<winlane::features::snippets::Snippet> = Vec::new();
        let extra_count = command_matches.len() + snippet_matches.len();
        let apps = self.ivars().installed_apps.borrow();
        let launch_matches = if self.ivars().mode.get() == Some(PanelMode::Search)
            && !self.ivars().demo.get()
            && scope_pid.is_none()
        {
            let identities = self.ivars().identities.borrow();
            let occupied: HashSet<_> = windows
                .iter()
                .filter_map(|window| identities.get(&window.pid))
                .map(|app| app.id.clone())
                .collect();
            matching_apps(
                &apps,
                &query,
                &occupied,
                &self.ivars().config.borrow().excluded_apps,
                aliases.resolve(&query),
            )
        } else {
            Vec::new()
        };
        drop(aliases);
        let selected = match selected_id {
            Some(SelectedResult::Command(id)) => {
                command_matches.iter().position(|item| *item == id)
            }
            Some(SelectedResult::Snippet(id)) => snippet_matches
                .iter()
                .position(|snippet| snippet.id == id)
                .map(|index| command_matches.len() + index),
            Some(SelectedResult::Quicklink(id)) => quicklink_matches
                .iter()
                .position(|q| q.id == id)
                .map(|i| extra_count + matched.len() + i),
            Some(SelectedResult::Window(id)) => matched
                .iter()
                .position(|&index| windows[index].id == id)
                .map(|index| extra_count + index),
            Some(SelectedResult::Application(id)) => launch_matches
                .iter()
                .position(|&index| apps[index].target.bundle_id == id)
                .map(|index| extra_count + matched.len() + quicklink_matches.len() + index)
                .or_else(|| {
                    let identities = self.ivars().identities.borrow();
                    matched
                        .iter()
                        .position(|&index| {
                            identities
                                .get(&windows[index].pid)
                                .is_some_and(|app| app.id == id)
                        })
                        .map(|index| extra_count + index)
                }),
            Some(
                SelectedResult::File(_)
                | SelectedResult::KeepAwake(_)
                | SelectedResult::Bluetooth(_)
                | SelectedResult::Clipboard(_)
                | SelectedResult::Project(_)
                | SelectedResult::OpenUrl(_)
                | SelectedResult::Emoji(_),
            )
            | None => None,
        }
        .unwrap_or(0);
        let visible_paths: HashSet<_> = launch_matches
            .iter()
            .map(|&index| apps[index].target.path.as_str())
            .collect();
        self.ivars()
            .application_icons
            .borrow_mut()
            .retain(|path, _| visible_paths.contains(path.as_str()));
        drop(apps);
        drop(windows);
        self.ivars().matches.replace(matched);
        self.ivars().launch_matches.replace(launch_matches);
        self.ivars().command_matches.replace(command_matches);
        self.ivars().snippet_matches.replace(snippet_matches);
        self.ivars().quicklink_matches.replace(quicklink_matches);
        self.ivars().clipboard_matches.borrow_mut().clear();
        self.ivars().selected.set(selected);
        self.render();
    }

    pub(super) fn move_selection(&self, direction: isize) {
        if self.editing_quicklink() {
            return;
        }
        if self.ivars().mode.get() == Some(PanelMode::Switch) {
            self.ivars().alias_input.borrow_mut().clear();
            if let Some(selection) = self.ivars().switch_selection.borrow_mut().as_mut() {
                selection.step(direction.signum() as i8);
                if let Some(index) = selection.selected() {
                    self.ivars().selected.set(index);
                }
            }
            self.render();
            return;
        }
        let count = self.match_count();
        if count == 0 {
            return;
        }
        self.ivars().selected.set(
            (self.ivars().selected.get() as isize + direction).rem_euclid(count as isize) as usize,
        );
        self.render();
    }

    pub(super) fn selected_window(&self) -> Option<WindowInfo> {
        let state = self.ivars();
        let index = state.selected.get().checked_sub(
            state.command_matches.borrow().len()
                + state.snippet_matches.borrow().len()
                + state.clipboard_matches.borrow().len(),
        )?;
        state
            .matches
            .borrow()
            .get(index)
            .and_then(|&index| state.windows.borrow().get(index).cloned())
    }

    pub(super) fn selected_application(&self) -> Option<ApplicationTarget> {
        let state = self.ivars();
        let index = state.selected.get().checked_sub(
            state.command_matches.borrow().len()
                + state.snippet_matches.borrow().len()
                + state.clipboard_matches.borrow().len()
                + state.matches.borrow().len()
                + state.quicklink_matches.borrow().len(),
        )?;
        state.launch_matches.borrow().get(index).and_then(|&index| {
            state
                .installed_apps
                .borrow()
                .get(index)
                .map(|app| app.target.clone())
        })
    }

    pub(super) fn selected_result(&self) -> Option<SelectedResult> {
        if self.searching_emoji() {
            return self
                .selected_emoji()
                .map(|emoji| SelectedResult::Emoji(emoji.text));
        }
        if self.searching_files() {
            return self
                .selected_file()
                .map(|entry| SelectedResult::File(entry.path));
        }
        if self.searching_keep_awake() {
            return self
                .ivars()
                .keep_awake_matches
                .borrow()
                .get(self.ivars().selected.get())
                .copied()
                .map(SelectedResult::KeepAwake);
        }
        if let Some(device) = self.selected_bluetooth() {
            return Some(SelectedResult::Bluetooth(device.address));
        }
        if let Some(page) = self.selected_url() {
            return Some(SelectedResult::OpenUrl(page.url));
        }
        if let Some(project) = self.selected_project() {
            return Some(SelectedResult::Project(project.path));
        }
        if let Some(link) = self.selected_quicklink() {
            return Some(SelectedResult::Quicklink(link.id));
        }
        if let Some(entry) = self.selected_clipboard() {
            return Some(SelectedResult::Clipboard(entry.id));
        }
        if let Some(snippet) = self.selected_snippet() {
            return Some(SelectedResult::Snippet(snippet.id));
        }
        if let Some(command) = self.selected_command() {
            return Some(SelectedResult::Command(command));
        }
        self.selected_window()
            .map(|window| SelectedResult::Window(window.id))
            .or_else(|| {
                self.selected_application()
                    .map(|app| SelectedResult::Application(app.bundle_id))
            })
    }

    pub(super) fn match_count(&self) -> usize {
        self.ivars().emoji_matches.borrow().len()
            + self.ivars().files.borrow().matches.len()
            + self.ivars().keep_awake_matches.borrow().len()
            + self.ivars().bluetooth_matches.borrow().len()
            + self.ivars().command_matches.borrow().len()
            + self.ivars().snippet_matches.borrow().len()
            + self.ivars().clipboard_matches.borrow().len()
            + self.ivars().matches.borrow().len()
            + self.ivars().launch_matches.borrow().len()
            + self.ivars().quicklink_matches.borrow().len()
            + self.ivars().project_matches.borrow().len()
            + self.ivars().open_url_matches.borrow().len()
    }
}
