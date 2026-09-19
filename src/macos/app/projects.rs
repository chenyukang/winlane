use super::*;

pub(super) enum ProjectUpdate {
    Cached(Vec<winlane::features::projects::Project>),
    Refreshed(winlane::features::projects::Cache),
}

impl Delegate {
    pub(super) fn preload_projects(&self) {
        self.start_project_refresh(false, true);
    }

    pub(super) fn refresh_projects(&self, force: bool) {
        if !self.searching_projects() {
            return;
        }
        self.cancel_scoped_refresh();
        self.start_project_refresh(force, false);
    }

    fn start_project_refresh(&self, force: bool, restore: bool) {
        let state = self.ivars();
        if state.project_receiver.borrow().is_some() {
            return;
        }
        let Some(home) = std::env::var_os("HOME") else {
            state.project_cache.borrow_mut().error =
                Some(tr!("找不到用户主目录。", "Home directory unavailable.").into());
            return;
        };
        let mut cache = state.project_cache.borrow().clone();
        let (tx, rx) = mpsc::channel();
        state.project_receiver.replace(Some(rx));
        let wake = state.wake.get().unwrap().handle();
        std::thread::spawn(move || {
            let snapshot = winlane::features::projects::snapshot::path(std::path::Path::new(&home));
            if restore && let Ok(projects) = winlane::features::projects::snapshot::load(&snapshot)
            {
                cache.projects = projects.clone();
                let _ = tx.send(ProjectUpdate::Cached(projects));
                wake.signal();
            }
            let application = objc2::rc::autoreleasepool(|_| {
                crate::macos::platform::project_open::application_path()
            });
            let sources = winlane::features::projects::Sources::vscode(
                std::path::Path::new(&home),
                application.as_deref(),
            );
            if cache.refresh(&sources, force) && cache.error.is_none() {
                let _ = winlane::features::projects::snapshot::save(&snapshot, &cache.projects);
            }
            let _ = tx.send(ProjectUpdate::Refreshed(cache));
            wake.signal();
        });
    }

    pub(super) fn poll_projects(&self) {
        let state = self.ivars();
        let selected = self.selected_result();
        let mut changed = false;
        loop {
            let result = state
                .project_receiver
                .borrow()
                .as_ref()
                .map(|rx| rx.try_recv());
            match result {
                Some(Ok(ProjectUpdate::Cached(projects))) => {
                    state.project_cache.borrow_mut().projects = projects;
                    changed = true;
                }
                Some(Ok(ProjectUpdate::Refreshed(cache))) => {
                    state.project_receiver.take();
                    state.project_cache.replace(cache);
                    changed = true;
                    break;
                }
                Some(Err(TryRecvError::Disconnected)) => {
                    state.project_receiver.take();
                    state.project_cache.borrow_mut().error = Some(
                        tr!(
                            "项目读取中断，按 ⌘R 重试。",
                            "Project loading stopped. Press ⌘R to retry."
                        )
                        .into(),
                    );
                    changed = true;
                    break;
                }
                _ => break,
            }
        }
        if changed && self.searching_projects() {
            self.filter_preserving(selected);
        }
    }

    pub(super) fn filter_projects(&self, selected: Option<SelectedResult>) {
        let state = self.ivars();
        let projects = winlane::features::projects::matching(
            &state.project_cache.borrow().projects,
            &state.query.borrow(),
        );
        let selected = if let Some(SelectedResult::Project(path)) = selected {
            projects
                .iter()
                .position(|project| project.path == path)
                .unwrap_or(0)
        } else {
            0
        };
        state.matches.borrow_mut().clear();
        state.launch_matches.borrow_mut().clear();
        state.command_matches.borrow_mut().clear();
        state.snippet_matches.borrow_mut().clear();
        state.clipboard_matches.borrow_mut().clear();
        state.quicklink_matches.borrow_mut().clear();
        state.application_icons.borrow_mut().clear();
        state.project_matches.replace(projects);
        state.selected.set(selected);
        self.render();
    }

    pub(super) fn selected_project(&self) -> Option<winlane::features::projects::Project> {
        if !self.searching_projects() {
            return None;
        }
        self.ivars()
            .project_matches
            .borrow()
            .get(self.ivars().selected.get())
            .cloned()
    }

    pub(super) fn open_selected_project(&self) {
        let Some(project) = self.selected_project() else {
            return;
        };
        match crate::macos::platform::project_open::PreparedProject::new(&project) {
            Ok(prepared) => {
                self.cancel_routing();
                self.end_session();
                self.ivars().launch_receiver.take();
                self.ivars().project_open.replace(Some(prepared.open(
                    self.ivars().previous_pid.get(),
                    self.ivars().wake.get().unwrap().handle(),
                )));
            }
            Err(error) => {
                self.ivars().project_cache.borrow_mut().error = Some(error);
                self.render();
            }
        }
    }

    pub(super) fn cancel_project_open_if_switched(&self, pid: i32, is_vscode: bool) {
        let keep = self
            .ivars()
            .project_open
            .borrow_mut()
            .as_mut()
            .is_none_or(|pending| {
                winlane::features::projects::focus::is_origin_or_target(
                    pending.origin_pid,
                    pid,
                    is_vscode,
                    &mut pending.target_seen,
                )
            });
        if !keep {
            self.ivars().project_open.take();
        }
    }

    pub(super) fn poll_project_open(&self) {
        if self.ivars().project_open.borrow().is_none() {
            return;
        }
        if self.ivars().mode.get().is_some() {
            self.ivars().project_open.take();
            return;
        }
        if let Some(front) = NSWorkspace::sharedWorkspace().frontmostApplication()
            && front.processIdentifier() != std::process::id() as i32
            && front.activationPolicy() == NSApplicationActivationPolicy::Regular
        {
            self.cancel_project_open_if_switched(
                front.processIdentifier(),
                front
                    .bundleIdentifier()
                    .is_some_and(|id| id.to_string() == "com.microsoft.VSCode"),
            );
        }
        let result = self
            .ivars()
            .project_open
            .borrow()
            .as_ref()
            .map(|pending| pending.receiver.try_recv());
        let result = match result {
            Some(Ok(result)) => result,
            Some(Err(TryRecvError::Disconnected)) => {
                self.ivars().project_open.take();
                return;
            }
            _ => return,
        };
        self.ivars().project_open.take();
        let opened = match result {
            Ok(opened) => opened,
            Err(error) => {
                self.report_launch_error(&error, LaunchOrigin::Search);
                return;
            }
        };
        let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(opened.pid)
        else {
            return;
        };
        if app
            .bundleIdentifier()
            .is_none_or(|id| id.to_string() != "com.microsoft.VSCode")
        {
            return;
        }
        app.unhide();
        if let Some(window) = opened.window {
            if accessibility::raise_window(opened.pid, window).is_err() {
                return;
            }
            if self.activate_app(&app) {
                self.remember_window(window);
            }
        } else {
            // Unknown/custom titles must never select an arbitrary Code window.
            self.activate_app(&app);
        }
    }
}
