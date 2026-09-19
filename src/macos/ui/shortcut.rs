use super::controls::{checkbox, popup, rect, set_action};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSString};
use winlane::core::config::{KEYS, Shortcut, key_label};
use winlane::tr;

pub(crate) struct ShortcutControls {
    pub(super) modifiers: [Retained<NSButton>; 4],
    pub(super) key: Retained<NSPopUpButton>,
    keys: Vec<&'static str>,
}

impl ShortcutControls {
    pub(crate) fn optional_at(view: &NSView, x: f64, y: f64, mtm: MainThreadMarker) -> Self {
        let mut controls = Self::at(view, y, false, mtm);
        for (i, (control, title)) in controls
            .modifiers
            .iter()
            .zip(["⌃", "⌥", "⇧", "⌘"])
            .enumerate()
        {
            control.setTitle(&NSString::from_str(title));
            control.setFrame(rect(x + i as f64 * 72.0, y, 68.0, 25.0));
            control.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        }
        controls.key.setFrame(rect(x + 298.0, y - 2.0, 190.0, 28.0));
        controls
            .key
            .setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        controls.keys.insert(0, "");
        controls
            .key
            .insertItemWithTitle_atIndex(&NSString::from_str(tr!("未设置", "Not set")), 0);
        controls
    }

    pub(crate) fn fill_optional(&self, shortcut: Option<&Shortcut>) {
        if let Some(shortcut) = shortcut {
            self.fill(shortcut);
        } else {
            for control in &self.modifiers {
                control.setState(NSControlStateValueOff);
            }
            self.key.selectItemAtIndex(0);
        }
    }

    pub(crate) fn read_optional(&self) -> Result<Option<Shortcut>, String> {
        if self.keys.get(self.key.indexOfSelectedItem() as usize) == Some(&"") {
            Ok(None)
        } else {
            self.read().map(Some)
        }
    }

    pub(crate) fn set_enabled(&self, enabled: bool) {
        for control in &self.modifiers {
            control.setEnabled(enabled);
        }
        self.key.setEnabled(enabled);
    }

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
            .filter(|key| !key.is_empty())
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
