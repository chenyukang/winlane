pub const KEY_DOWN: u32 = 10;
pub const KEY_UP: u32 = 11;
pub const FLAGS_CHANGED: u32 = 12;
pub const SHIFT: u64 = 1 << 17;
pub const CONTROL: u64 = 1 << 18;
pub const OPTION: u64 = 1 << 19;
pub const COMMAND: u64 = 1 << 20;
pub const TAB: i64 = 48;
pub const ESCAPE: i64 = 53;

pub fn letter_for_key(key: i64) -> Option<char> {
    Some(match key {
        0 => 'a',
        1 => 's',
        2 => 'd',
        3 => 'f',
        4 => 'h',
        5 => 'g',
        6 => 'z',
        7 => 'x',
        8 => 'c',
        9 => 'v',
        11 => 'b',
        12 => 'q',
        13 => 'w',
        14 => 'e',
        15 => 'r',
        16 => 'y',
        17 => 't',
        31 => 'o',
        32 => 'u',
        34 => 'i',
        35 => 'p',
        37 => 'l',
        38 => 'j',
        40 => 'k',
        45 => 'n',
        46 => 'm',
        _ => return None,
    })
}

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

pub const SPACE: i64 = 49;
const MODIFIERS: u64 = COMMAND | CONTROL | OPTION | SHIFT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelMode {
    Search,
    Switch,
}

pub fn space_changes_mode(mode: PanelMode, query_empty: bool, composing: bool) -> bool {
    !composing && (mode == PanelMode::Switch || query_empty)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub key: i64,
    pub modifiers: u64,
}

impl Binding {
    fn matches(self, key: i64, flags: u64, reverse: bool) -> bool {
        let flags = flags & MODIFIERS;
        key == self.key
            && (flags == self.modifiers
                || (reverse && self.modifiers & SHIFT == 0 && flags == self.modifiers | SHIFT))
    }

    pub fn primary_modifier(self) -> u64 {
        [COMMAND, CONTROL, OPTION]
            .into_iter()
            .find(|flag| self.modifiers & flag != 0)
            .unwrap_or(0)
    }

    pub fn conflicts_with_switch(self, switch: Self) -> bool {
        switch.matches(self.key, self.modifiers, true)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Search,
    Switch { direction: i8, fresh: bool },
    Alias(char),
    AliasBackspace,
    Accept,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Action {
    pub session: u64,
    pub kind: ActionKind,
}

pub struct ShortcutRouter {
    search: Binding,
    switch: Binding,
    session: u64,
    mode: Option<PanelMode>,
    release_modifier: Option<u64>,
    consumed: std::collections::BTreeSet<i64>,
}

impl ShortcutRouter {
    pub fn new(search: Binding, switch: Binding) -> Self {
        Self {
            search,
            switch,
            session: 0,
            mode: None,
            release_modifier: None,
            consumed: Default::default(),
        }
    }

    fn action(&self, kind: ActionKind) -> Action {
        Action {
            session: self.session,
            kind,
        }
    }

    fn begin(&mut self, mode: PanelMode) {
        if self.mode.is_none() {
            self.session = self.session.wrapping_add(1);
        }
        self.mode = Some(mode);
    }

    pub fn open_search(&mut self) -> Action {
        self.begin(PanelMode::Search);
        self.release_modifier = None;
        self.action(ActionKind::Search)
    }

    pub fn resume_search(&mut self, session: u64) {
        if self.session == session {
            self.mode = Some(PanelMode::Search);
            self.release_modifier = None;
        }
    }

    pub fn enter_switch(&mut self, flags: u64) -> Action {
        self.begin(PanelMode::Switch);
        let primary = self.switch.primary_modifier();
        self.release_modifier = (flags & primary != 0).then_some(primary);
        self.action(ActionKind::Switch {
            direction: 0,
            fresh: true,
        })
    }

    pub fn finish(&mut self, session: u64) {
        if self.session == session {
            self.mode = None;
            self.release_modifier = None;
        }
    }

    pub fn cancel(&mut self) -> Action {
        let action = self.action(ActionKind::Cancel);
        self.finish(self.session);
        action
    }

    pub fn handle(
        &mut self,
        event: u32,
        key: i64,
        flags: u64,
        repeat: bool,
    ) -> (bool, Option<Action>) {
        if event == FLAGS_CHANGED {
            if self.mode == Some(PanelMode::Switch) {
                if self
                    .release_modifier
                    .is_some_and(|modifier| flags & modifier == 0)
                {
                    let action = self.action(ActionKind::Accept);
                    self.finish(self.session);
                    return (false, Some(action));
                }
                let primary = self.switch.primary_modifier();
                if self.release_modifier.is_none() && flags & primary != 0 {
                    self.release_modifier = Some(primary);
                }
            }
            return (false, None);
        }
        if event == KEY_UP {
            return (self.consumed.remove(&key), None);
        }
        if event != KEY_DOWN {
            return (false, None);
        }
        if self.consumed.contains(&key) {
            return (true, None);
        }
        let alias_key = self.mode == Some(PanelMode::Switch)
            && (flags & MODIFIERS & !SHIFT == 0
                || flags & MODIFIERS & !SHIFT == self.switch.modifiers & !SHIFT)
            && (letter_for_key(key).is_some() || key == 51);
        if repeat {
            let consume = self.search.matches(key, flags, false)
                || self.switch.matches(key, flags, true)
                || (self.mode == Some(PanelMode::Switch)
                    && matches!(key, SPACE | ESCAPE | 36 | 76 | TAB | 125 | 126))
                || alias_key;
            if consume {
                self.consumed.insert(key);
            }
            return (consume, None);
        }
        let action = if alias_key && !self.switch.matches(key, flags, true) {
            self.action(letter_for_key(key).map_or(ActionKind::AliasBackspace, ActionKind::Alias))
        } else if self.search.matches(key, flags, false) {
            if self.mode == Some(PanelMode::Search) {
                self.cancel()
            } else {
                self.open_search()
            }
        } else if self.switch.matches(key, flags, true) {
            let fresh = self.mode != Some(PanelMode::Switch);
            self.enter_switch(flags);
            self.action(ActionKind::Switch {
                direction: if self.switch.modifiers & SHIFT == 0 && flags & SHIFT != 0 {
                    -1
                } else {
                    1
                },
                fresh,
            })
        } else if self.mode == Some(PanelMode::Switch) {
            match key {
                SPACE => self.open_search(),
                ESCAPE => self.cancel(),
                36 | 76 => {
                    let action = self.action(ActionKind::Accept);
                    self.finish(self.session);
                    action
                }
                TAB | 125 | 126 => self.action(ActionKind::Switch {
                    direction: if key == 126 || (key == TAB && flags & SHIFT != 0) {
                        -1
                    } else {
                        1
                    },
                    fresh: false,
                }),
                _ => return (false, None),
            }
        } else {
            return (false, None);
        };
        self.consumed.insert(key);
        (true, Some(action))
    }
}

#[derive(Debug)]
pub struct SwitchSelection {
    len: usize,
    index: usize,
    pending_steps: i64,
    released: bool,
    committed: bool,
}

impl SwitchSelection {
    pub fn new(direction: i8) -> Self {
        Self {
            len: 0,
            index: 0,
            pending_steps: i64::from(direction),
            released: false,
            committed: false,
        }
    }

    pub fn install(&mut self, len: usize, anchor: Option<usize>) {
        if self.len != 0 || len == 0 {
            return;
        }
        self.len = len;
        self.index = anchor
            .filter(|index| *index < len)
            .unwrap_or(if self.pending_steps > 0 { len - 1 } else { 0 });
        self.index = (self.index as i64 + self.pending_steps.rem_euclid(len as i64))
            .rem_euclid(len as i64) as usize;
        self.pending_steps = 0;
    }

    pub fn step(&mut self, direction: i8) {
        if self.len == 0 {
            self.pending_steps = self.pending_steps.saturating_add(i64::from(direction));
        } else {
            self.index =
                (self.index as i64 + i64::from(direction)).rem_euclid(self.len as i64) as usize;
        }
    }

    pub fn selected(&self) -> Option<usize> {
        (self.len > 0).then_some(self.index)
    }

    pub fn select(&mut self, index: usize) {
        if index < self.len {
            self.index = index;
        }
    }
    pub fn release(&mut self) {
        self.released = true;
    }
    pub fn released(&self) -> bool {
        self.released
    }

    pub fn take_commit(&mut self) -> Option<usize> {
        if self.released && !self.committed && self.len > 0 {
            self.committed = true;
            Some(self.index)
        } else {
            None
        }
    }
}
