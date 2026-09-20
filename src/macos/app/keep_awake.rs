use super::*;

impl Delegate {
    pub(super) fn update_keep_awake_indicator(&self, force: bool) -> bool {
        let status = self
            .ivars()
            .keep_awake
            .borrow()
            .indicator_status(std::time::SystemTime::now());
        let mut indicator = self.ivars().keep_awake_indicator.borrow_mut();
        let previous = indicator.as_ref().and_then(|indicator| indicator.status);
        let changed = previous != status;
        let Some(status) = status else {
            indicator.take();
            return changed;
        };
        // The existing heartbeat checks time; redraw only when the minute or display changes.
        if !force && !changed {
            return false;
        }
        let occupied = self
            .ivars()
            .input_indicator
            .borrow()
            .as_ref()
            .map_or_else(Vec::new, |input| input.frames());
        let screens = crate::macos::ui::input_indicator::screens(self.mtm());
        let indicator = indicator.get_or_insert_with(Default::default);
        indicator.configure(status, &screens, &occupied, self.mtm());
        indicator.show();
        changed
    }

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
        self.update_keep_awake_indicator(true);
        self.render();
    }
}
