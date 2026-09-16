use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputMethod {
    #[default]
    Current,
    English,
    Chinese,
    LastUsed,
}

#[derive(Default)]
pub struct InputSession {
    pub focused: bool,
    pub selected: Option<String>,
    previous: Option<String>,
    restore: bool,
}

impl InputSession {
    pub fn prepare(&mut self, current: Option<String>, policy: InputMethod) {
        *self = Self {
            previous: current,
            restore: policy != InputMethod::Current,
            ..Self::default()
        };
    }

    pub fn observe(&mut self, current: Option<String>) -> bool {
        if !self.focused || current.is_none() || self.selected == current {
            return false;
        }
        self.selected = current;
        true
    }

    pub fn finish(&mut self, current: Option<&str>) -> Option<String> {
        let restore = self.focused
            && self.restore
            && current.is_some()
            && current == self.selected.as_deref()
            && self.previous != self.selected;
        let previous = self.previous.take().filter(|_| restore);
        *self = Self::default();
        previous
    }
}
