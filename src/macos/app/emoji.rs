use super::*;
use winlane::features::emoji::Emoji;

impl Delegate {
    pub(super) fn filter_emoji(&self, selected: Option<SelectedResult>) {
        self.cancel_files_search();
        let state = self.ivars();
        let matches = winlane::features::emoji::matching(&state.query.borrow());
        let index = if let Some(SelectedResult::Emoji(text)) = selected {
            matches
                .iter()
                .position(|emoji| emoji.text == text)
                .unwrap_or(0)
        } else {
            0
        };
        self.clear_result_matches();
        state.emoji_matches.replace(matches);
        state.selected.set(index);
        self.render();
    }

    pub(super) fn selected_emoji(&self) -> Option<Emoji> {
        if !self.searching_emoji() {
            return None;
        }
        self.ivars()
            .emoji_matches
            .borrow()
            .get(self.ivars().selected.get())
            .copied()
    }

    pub(super) fn use_emoji(&self) {
        let Some(emoji) = self.selected_emoji() else {
            return;
        };
        let Some(target) = NSRunningApplication::runningApplicationWithProcessIdentifier(
            self.ivars().previous_pid.get(),
        ) else {
            self.report_switch_error(tr!(
                "目标应用已退出，请重新打开搜索。",
                "The target app has quit. Reopen search."
            ));
            return;
        };
        self.cancel_routing();
        self.end_session();
        if let Err(error) = self.paste_text(target, emoji.text.into()) {
            self.selection_failed(&error);
        }
    }
}
