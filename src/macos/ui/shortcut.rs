use super::controls::{checkbox, popup, rect, set_action};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2_app_kit::*;
use objc2_foundation::MainThreadMarker;
use winlane::core::config::{KEYS, Shortcut, key_label};
use winlane::tr;

pub(crate) struct ShortcutControls {
    pub(super) modifiers: [Retained<NSButton>; 4],
    pub(super) key: Retained<NSPopUpButton>,
    keys: Vec<&'static str>,
}

impl ShortcutControls {
    pub(crate) fn at(view: &NSView, y: f64, switching: bool, mtm: MainThreadMarker) -> Self {
        let modifiers =
            ["⌃ Control", "⌥ Option", "⇧ Shift", "⌘ Command"].map(|title| checkbox(title, mtm));
        for (i, control) in modifiers.iter().enumerate() {
            control.setFrame(rect(30.0 + i as f64 * 110.0, y, 108.0, 25.0));
            view.addSubview(control);
        }
        let keys: Vec<_> = KEYS
            .iter()
            .copied()
            .filter(|key| !switching || *key != "Space")
            .collect();
        let key = popup(
            &keys.iter().map(|key| key_label(key)).collect::<Vec<_>>(),
            rect(480.0, y - 2.0, 150.0, 28.0),
            mtm,
        );
        view.addSubview(&key);
        Self {
            modifiers,
            key,
            keys,
        }
    }

    pub(crate) fn fill(&self, shortcut: &Shortcut) {
        for (control, enabled) in self.modifiers.iter().zip([
            shortcut.control,
            shortcut.option,
            shortcut.shift,
            shortcut.command,
        ]) {
            control.setState(if enabled {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
        self.key.selectItemAtIndex(
            self.keys
                .iter()
                .position(|key| *key == shortcut.key)
                .unwrap_or(0) as isize,
        );
    }

    pub(crate) fn read(&self) -> Result<Shortcut, String> {
        let [control, option, shift, command] =
            std::array::from_fn(|i| self.modifiers[i].state() == NSControlStateValueOn);
        let key = self
            .keys
            .get(self.key.indexOfSelectedItem() as usize)
            .ok_or(tr!("请选择快捷键。", "Choose a shortcut."))?;
        Ok(Shortcut {
            control,
            option,
            shift,
            command,
            key: (*key).into(),
        })
    }

    pub(crate) fn copy_from(&self, other: &Self) {
        for (control, previous) in self.modifiers.iter().zip(&other.modifiers) {
            control.setState(previous.state());
        }
        self.key.selectItemAtIndex(other.key.indexOfSelectedItem());
    }

    pub(crate) fn on_change(&self, target: &AnyObject, action: Sel) {
        for control in &self.modifiers {
            set_action(control, target, action);
        }
        set_action(&self.key, target, action);
    }

    pub(crate) fn notify_changed(&self) {
        // SAFETY: on_change installs a live delegate and its matching action.
        unsafe {
            self.key
                .sendAction_to(self.key.action(), self.key.target().as_deref())
        };
    }
}
