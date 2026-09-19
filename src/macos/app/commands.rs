use super::*;

impl Delegate {
    pub(super) fn selected_command(&self) -> Option<CommandId> {
        self.ivars()
            .command_matches
            .borrow()
            .get(self.ivars().selected.get())
            .copied()
    }

    pub(super) fn remember_panel_display(&self, window: &NSWindow) {
        if let Some(ui) = self.panels().iter().find(|ui| {
            let panel: &NSWindow = &ui.panel;
            std::ptr::eq(panel, window)
        }) {
            self.ivars().keyboard_display.set(Some(ui.display_id));
        }
    }

    pub(super) fn execute_command(&self, command: CommandId) {
        match crate::macos::platform::system_commands::PreparedCommand::prepare(
            command,
            self.ivars().keyboard_display.get(),
            self.mtm(),
        ) {
            Ok(prepared) => {
                if matches!(
                    command,
                    CommandId::ShowMenu | CommandId::Screenshot | CommandId::ToggleAppearance
                ) {
                    self.dismiss();
                } else {
                    // Reactivating an app afterward would interrupt Mission Control or locking.
                    self.cancel_routing();
                    self.end_session();
                }
                if let Err(error) = prepared.execute() {
                    self.selection_failed(&error);
                }
            }
            Err(error) => self.selection_failed(&error),
        }
    }
}
