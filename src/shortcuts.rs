pub const KEY_DOWN: u32 = 10;
pub const KEY_UP: u32 = 11;
pub const FLAGS_CHANGED: u32 = 12;
pub const SHIFT: u64 = 1 << 17;
pub const CONTROL: u64 = 1 << 18;
pub const OPTION: u64 = 1 << 19;
pub const COMMAND: u64 = 1 << 20;
pub const TAB: i64 = 48;
pub const ESCAPE: i64 = 53;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelCommand {
    Next,
    Previous,
    Accept,
    Cancel,
}

pub fn panel_command(key: i64, flags: u64, composing: bool) -> Option<PanelCommand> {
    let modifiers = flags & (COMMAND | CONTROL | OPTION | SHIFT);
    if composing || modifiers & (CONTROL | OPTION) != 0 {
        return None;
    }
    match (key, modifiers) {
        (125, 0 | COMMAND) | (TAB, 0) => Some(PanelCommand::Next),
        (126, 0 | COMMAND) | (TAB, SHIFT) => Some(PanelCommand::Previous),
        (36 | 76, 0) => Some(PanelCommand::Accept),
        (ESCAPE, 0) => Some(PanelCommand::Cancel),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandTabAction {
    Next,
    Previous,
    Cancel,
}

#[derive(Default)]
pub struct PressLatch(bool);

impl PressLatch {
    pub fn update(&mut self, pressed: bool) -> bool {
        let first = pressed && !self.0;
        self.0 = pressed;
        first
    }
}

#[derive(Default)]
pub struct CommandTabState {
    tab_down: bool,
    escape_down: bool,
    session: bool,
}

impl CommandTabState {
    pub fn handle(
        &mut self,
        event_type: u32,
        key: i64,
        flags: u64,
        repeat: bool,
    ) -> (bool, Option<CommandTabAction>) {
        if event_type == FLAGS_CHANGED {
            if flags & COMMAND == 0 {
                self.session = false;
            }
            return (false, None);
        }
        if event_type == KEY_UP {
            let swallowed = match key {
                TAB => std::mem::take(&mut self.tab_down),
                ESCAPE => std::mem::take(&mut self.escape_down),
                _ => false,
            };
            return (swallowed, None);
        }
        if event_type != KEY_DOWN {
            return (false, None);
        }
        let command_only = flags & (COMMAND | CONTROL | OPTION) == COMMAND;
        if key == TAB {
            if !command_only {
                self.tab_down = false;
                return (false, None);
            }
            let first = !repeat && !self.tab_down;
            self.tab_down = true;
            self.session = true;
            let action = if flags & SHIFT != 0 {
                CommandTabAction::Previous
            } else {
                CommandTabAction::Next
            };
            return (true, first.then_some(action));
        }
        if key == ESCAPE && self.escape_down {
            return (true, None);
        }
        if key == ESCAPE && self.session && command_only {
            self.escape_down = true;
            self.session = false;
            return (true, Some(CommandTabAction::Cancel));
        }
        (false, None)
    }
}
