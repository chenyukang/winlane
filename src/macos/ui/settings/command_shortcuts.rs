use super::*;
use winlane::core::commands::{COMMANDS, CommandId};
use winlane::core::config::CommandShortcut;

pub(super) struct CommandShortcutControls {
    pub(super) view: Retained<NSView>,
    pub(super) rows: Vec<(CommandId, ShortcutControls)>,
}

impl CommandShortcutControls {
    pub(super) fn new(parent: &NSView, target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let height = 48.0 + COMMANDS.len() as f64 * 66.0;
        let view = card(parent, rect(0.0, 0.0, 740.0, height), mtm);
        view.addSubview(&label(
            tr!("命令快捷键", "Command shortcuts"),
            15.0,
            rect(24.0, height - 34.0, 690.0, 25.0),
            mtm,
        ));
        let mut rows = Vec::new();
        for (index, command) in COMMANDS.iter().enumerate() {
            let y = height - 80.0 - index as f64 * 66.0;
            view.addSubview(&label(command.name, 14.0, rect(24.0, y, 190.0, 25.0), mtm));
            view.addSubview(&hint(
                command.title(),
                rect(24.0, y - 25.0, 690.0, 21.0),
                mtm,
            ));
            let shortcut = ShortcutControls::optional_at(&view, 228.0, y, mtm);
            shortcut.fill_optional(None);
            shortcut.on_change(target, sel!(settingsChanged:));
            rows.push((command.id, shortcut));
        }
        Self { view, rows }
    }

    pub(super) fn fill(&self, shortcuts: &[CommandShortcut]) {
        for (command, controls) in &self.rows {
            controls.fill_optional(
                shortcuts
                    .iter()
                    .find(|item| item.command == *command)
                    .map(|item| &item.shortcut),
            );
        }
    }

    pub(super) fn read(&self) -> Result<Vec<CommandShortcut>, String> {
        let mut result = Vec::new();
        for (command, controls) in &self.rows {
            if let Some(shortcut) = controls.read_optional()? {
                result.push(CommandShortcut {
                    command: *command,
                    shortcut,
                });
            }
        }
        Ok(result)
    }
}
