use super::*;

impl Delegate {
    pub(super) fn refresh_projects(&self, force: bool) {
        let state = self.ivars();
        if !self.searching_projects() || state.project_receiver.borrow().is_some() {
            return;
        }
        let Some(home) = std::env::var_os("HOME") else {
            state.project_cache.borrow_mut().error =
                Some(tr!("找不到用户主目录。", "Home directory unavailable.").into());
            return;
        };
        let application = crate::macos::platform::project_open::application_path();
        let mut cache = state.project_cache.borrow().clone();
        let (tx, rx) = mpsc::channel();
        state.project_receiver.replace(Some(rx));
        let wake = state.wake.get().unwrap().handle();
        std::thread::spawn(move || {
            let sources = winlane::features::projects::Sources::vscode(
                std::path::Path::new(&home),
                application.as_deref(),
            );
            cache.refresh(&sources, force);
            let _ = tx.send(cache);
            wake.signal();
        });
    }

    pub(super) fn poll_projects(&self) {
        let result = self
            .ivars()
            .project_receiver
            .borrow()
            .as_ref()
            .map(|rx| rx.try_recv());
        match result {
            Some(Ok(cache)) => {
                let selected = self.selected_result();
                self.ivars().project_receiver.take();
                self.ivars().project_cache.replace(cache);
                if self.searching_projects() {
                    self.filter_preserving(selected);
                }
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.ivars().project_receiver.take();
                self.ivars().project_cache.borrow_mut().error = Some(
                    tr!(
                        "项目读取中断，按 ⌘R 重试。",
                        "Project loading stopped. Press ⌘R to retry."
                    )
                    .into(),
                );
                if self.searching_projects() {
                    self.render();
                }
            }
            _ => {}
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
                let receiver = prepared.open(self.ivars().wake.get().unwrap().handle());
                self.ivars().launch_receiver.replace(Some(PendingLaunch {
                    receiver,
                    origin: LaunchOrigin::Search,
                }));
            }
            Err(error) => {
                self.ivars().project_cache.borrow_mut().error = Some(error);
                self.render();
            }
        }
    }
}
