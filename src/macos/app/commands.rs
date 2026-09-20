use super::*;

pub(super) fn command_scope(command: CommandId) -> Option<SearchScope> {
    match command {
        CommandId::Files => Some(SearchScope::Files),
        CommandId::KeepAwake => Some(SearchScope::KeepAwake),
        CommandId::Bluetooth => Some(SearchScope::Bluetooth),
        CommandId::OpenUrl => Some(SearchScope::OpenUrl),
        CommandId::Projects => Some(SearchScope::Projects),
        CommandId::Quicklinks => Some(SearchScope::Quicklinks),
        CommandId::Snippets => Some(SearchScope::Snippets),
        CommandId::Clipboard => Some(SearchScope::Clipboard),
        _ => None,
    }
}

impl Delegate {
    pub(super) fn run_command_shortcut(&self, command: CommandId, session: u64) {
        if !self
            .ivars()
            .config
            .borrow()
            .command_shortcuts
            .iter()
            .any(|item| item.command == command)
        {
            return;
        }
        if self.prepare_command_search(command, session) {
            self.present_panels();
            self.schedule_cache_warmup();
        } else {
            if self.ivars().mode.get().is_none() {
                self.capture_panel_origin();
                self.sync_displays();
            }
            if let Some(settings) = self.settings_window() {
                settings.window.orderOut(None);
            }
            self.execute_command(command);
        }
    }

    pub(super) fn prepare_command_search(&self, command: CommandId, session: u64) -> bool {
        let Some(scope) = command_scope(command) else {
            return false;
        };
        self.prepare_panel_in_scope(PanelMode::Search, session, 0, Some(scope));
        if let Some(tap) = self.ivars().shortcut_tap.borrow().as_ref() {
            tap.resume_search(session);
        }
        true
    }

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
