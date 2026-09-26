use super::*;

impl Delegate {
    pub(super) fn refresh_git_branch(&self) {
        if !self.searching_git_branch() {
            return;
        }
        self.cancel_scoped_refresh();
        let state = self.ivars();
        let pid = state.previous_pid.get();
        let window = state.previous_window.get();
        let projects = state.project_cache.borrow().projects.clone();
        state
            .git_branch_async
            .start(&state.wake.get().unwrap().handle(), move || {
                crate::macos::platform::git_branch::load(pid, window, projects)
            });
    }

    pub(super) fn poll_git_branch(&self) {
        let Some(result) = self.ivars().git_branch_async.poll() else {
            return;
        };
        let branches = result.unwrap_or_else(|()| winlane::features::git_branch::Branches {
            items: Vec::new(),
            error: Some(
                tr!(
                    "分支读取中断，按 ⌘R 重试。",
                    "Branch loading stopped. Press ⌘R to retry."
                )
                .into(),
            ),
            repo: None,
        });
        let state = self.ivars();
        let selected = self.selected_result();
        state.git_branch_results.replace(branches);
        if self.searching_git_branch() {
            self.filter_preserving(selected);
        }
    }

    pub(super) fn clear_git_branch_matches(&self) {
        self.ivars().git_branch_matches.borrow_mut().clear();
        self.clear_scope_rows(|content| matches!(content, RowContent::GitBranch(_)));
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
