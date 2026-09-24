use super::*;

impl Delegate {
    pub(super) fn refresh_git_branch(&self) {
        let state = self.ivars();
        if !self.searching_git_branch() {
            return;
        }
        self.cancel_scoped_refresh();
        if state.git_branch_receiver.borrow().is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        state.git_branch_receiver.replace(Some(rx));
        let pid = state.previous_pid.get();
        let window = state.previous_window.get();
        let projects = state.project_cache.borrow().projects.clone();
        let wake = state.wake.get().unwrap().handle();
        std::thread::spawn(move || {
            let _ = tx.send(crate::macos::platform::git_branch::load(
                pid, window, projects,
            ));
            wake.signal();
        });
    }

    pub(super) fn poll_git_branch(&self) {
        let result = self
            .ivars()
            .git_branch_receiver
            .borrow()
            .as_ref()
            .map(|rx| rx.try_recv());
        let branches = match result {
            Some(Ok(branches)) => branches,
            Some(Err(TryRecvError::Disconnected)) => winlane::features::git_branch::Branches {
                items: Vec::new(),
                error: Some(
                    tr!(
                        "分支读取中断，按 ⌘R 重试。",
                        "Branch loading stopped. Press ⌘R to retry."
                    )
                    .into(),
                ),
                repo: None,
            },
            _ => return,
        };
        let state = self.ivars();
        state.git_branch_receiver.take();
        let selected = self.selected_result();
        state.git_branch_results.replace(branches);
        if self.searching_git_branch() {
            self.filter_preserving(selected);
        }
    }

    pub(super) fn clear_git_branch_matches(&self) {
        self.ivars().git_branch_matches.borrow_mut().clear();
        for ui in self.panels() {
            for row in ui.rows.borrow_mut().iter_mut() {
                if matches!(row.content, Some(RowContent::GitBranch(_))) {
                    row.content = None;
                    row.app.setStringValue(&NSString::from_str(""));
                    row.title.setStringValue(&NSString::from_str(""));
                    row.button.setToolTip(None);
                    row.button.setAccessibilityLabel(None);
                }
            }
        }
    }

    pub(super) fn filter_git_branch(&self, selected: Option<SelectedResult>) {
        let state = self.ivars();
        let items = winlane::features::git_branch::matching(
            &state.git_branch_results.borrow().items,
            &state.query.borrow(),
        );
        let selected = if let Some(SelectedResult::GitBranch(name)) = selected {
            items.iter().position(|item| item.name == name)
        } else {
            None
        };
        self.clear_result_matches();
        state.git_branch_matches.replace(items);
        state.selected.set(selected.unwrap_or(0));
        self.render();
    }

    pub(super) fn selected_git_branch(&self) -> Option<winlane::features::git_branch::Branch> {
        if !self.searching_git_branch() {
            return None;
        }
        self.ivars()
            .git_branch_matches
            .borrow()
            .get(self.ivars().selected.get())
            .cloned()
    }

    pub(super) fn submit_git_branch(&self) {
        let Some(branch) = self.selected_git_branch() else {
            return;
        };
        let Some(repo) = self.ivars().git_branch_results.borrow().repo.clone() else {
            return;
        };
        // Switching to the branch already checked out here is a no-op; just leave.
        if branch.current {
            self.cancel_routing();
            self.dismiss();
            return;
        }
        match crate::macos::platform::git_branch::checkout(&repo, &branch.name) {
            Ok(()) => {
                self.cancel_routing();
                self.dismiss();
            }
            Err(error) => {
                self.ivars().git_branch_results.borrow_mut().error = Some(error);
                self.render();
            }
        }
    }
}
