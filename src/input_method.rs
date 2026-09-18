use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputMethod {
    Current,
    #[default]
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

/// Holds early key events until the requested input source is acknowledged.
pub struct InputGate<T> {
    target: Option<String>,
    events: Vec<T>,
}

impl<T> Default for InputGate<T> {
    fn default() -> Self {
        Self {
            target: None,
            events: Vec::new(),
        }
    }
}

impl<T> InputGate<T> {
    pub fn begin(&mut self, target: Option<String>) {
        self.target = target;
        self.events.clear();
    }

    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    pub fn push(&mut self, event: T) {
        self.events.push(event);
    }

    pub fn finish(&mut self, current: Option<&str>, expired: bool) -> Option<Vec<T>> {
        if !expired && self.target.is_some() && self.target.as_deref() != current {
            return None;
        }
        self.target = None;
        Some(std::mem::take(&mut self.events))
    }
}
