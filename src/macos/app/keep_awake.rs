use super::*;

impl Delegate {
    pub(super) fn filter_keep_awake(&self, selected: Option<SelectedResult>) {
        let state = self.ivars();
        let choices = winlane::features::keep_awake::matching(&state.query.borrow());
        let index = if let Some(SelectedResult::KeepAwake(choice)) = selected {
            choices.iter().position(|item| *item == choice)
        } else {
            None
        };
        state.matches.borrow_mut().clear();
        state.launch_matches.borrow_mut().clear();
        state.command_matches.borrow_mut().clear();
        state.snippet_matches.borrow_mut().clear();
        state.clipboard_matches.borrow_mut().clear();
        state.quicklink_matches.borrow_mut().clear();
        state.project_matches.borrow_mut().clear();
        state.bluetooth_matches.borrow_mut().clear();
        self.clear_open_url_matches();
        state.application_icons.borrow_mut().clear();
        state.keep_awake_matches.replace(choices);
        state.selected.set(index.unwrap_or(0));
        self.render();
    }

    pub(super) fn apply_keep_awake(&self) {
        let Some(SelectedResult::KeepAwake(choice)) = self.selected_result() else {
            return;
        };
        let result = self.ivars().keep_awake.borrow_mut().apply(choice);
        self.ivars().keep_awake_error.replace(result.err());
        self.render();
    }
}
