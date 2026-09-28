//! Key remapping: "this combination becomes that combination".
//!
//! Winlane watches every keystroke to run its own shortcuts. A rule here
//! rewrites a matching press before any application sees it, which is what a
//! keyboard remapper does — the difference is that this runs inside Winlane's
//! event tap instead of a virtual keyboard device.
//!
//! Rules are deliberately plain: one key plus the modifiers held with it, an
//! output combination, and an optional list of applications to leave alone.
//! Nothing here is timing dependent, so a rule cannot make typing feel wrong
//! the way a layer or chord could.

use crate::core::shortcuts::{COMMAND, CONTROL, KEY_DOWN, KEY_UP, OPTION, SHIFT};
use crate::{tr, trf};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The most rules one configuration may hold. Every keystroke is compared
/// against the list, and a list this size is already far past what a hand
/// written set needs.
pub const MAX_REMAPS: usize = 200;

/// One rule: pressing `from_key` with `from_modifiers` sends `to_key` with
/// `to_modifiers` instead, everywhere except in `except_apps`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyRemap {
    pub id: String,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    pub from_key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub from_modifiers: Vec<String>,
    /// Whether other modifiers may also be held while the rule still applies.
    /// Off means the listed modifiers have to be the whole set, so a rule for a
    /// plain key does not steal combinations like Shift plus that key.
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_extra_modifiers: bool,
    pub to_key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub to_modifiers: Vec<String>,
    /// Bundle identifiers the rule leaves alone: terminals, remote desktops and
    /// virtual machines usually need the original key.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub except_apps: Vec<String>,
}

fn enabled_by_default() -> bool {
    true
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl KeyRemap {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() || self.id.chars().count() > 100 {
            return Err(tr!("按键规则标识无效。", "Invalid key remap ID.").into());
        }
        for key in [&self.from_key, &self.to_key] {
            if key_code(key).is_none() {
                return Err(trf!(
                    "无法识别的按键：{key}",
                    "Unknown key: {key}",
                    key = key
                ));
            }
        }
        for modifiers in [&self.from_modifiers, &self.to_modifiers] {
            for modifier in modifiers {
                if modifier_flag(modifier).is_none() {
                    return Err(trf!(
                        "无法识别的修饰键：{modifier}",
                        "Unknown modifier: {modifier}",
                        modifier = modifier
                    ));
                }
            }
        }
        if self
            .except_apps
            .iter()
            .any(|app| app.trim().is_empty() || app.chars().any(char::is_control))
        {
            return Err(tr!(
                "排除的应用标识无效。",
                "Invalid application identifier in the exclusion list."
            )
            .into());
        }
        Ok(())
    }
}

/// Validate a whole rule list before it replaces the active configuration.
pub fn validate(remaps: &[KeyRemap]) -> Result<(), String> {
    if remaps.len() > MAX_REMAPS {
        return Err(trf!(
            "最多 200 条按键规则（当前 {count} 条）。",
            "At most 200 key remaps are supported (currently {count}).",
            count = remaps.len()
        ));
    }
    for remap in remaps {
        remap.validate()?;
    }
    let mut seen = std::collections::HashSet::new();
    for remap in remaps {
        if !remap.enabled {
            continue;
        }
        let key = (
            remap.from_key.to_lowercase(),
            remap
                .from_modifiers
                .iter()
                .map(|modifier| modifier.to_lowercase())
                .collect::<Vec<_>>()
                .join("+"),
            remap.allow_extra_modifiers,
        );
        if !seen.insert(key) {
            return Err(trf!(
                "有两条规则重复映射了同一个组合：{key}",
                "Two rules remap the same combination: {key}",
                key = remap.from_key
            ));
        }
    }
    Ok(())
}

/// The macOS virtual key code for a key name. Names follow Karabiner's spelling
/// so a rule can be read next to the configuration it came from.
pub fn key_code(name: &str) -> Option<i64> {
    Some(match normalized(name).as_str() {
        "a" => 0,
        "s" => 1,
        "d" => 2,
        "f" => 3,
        "h" => 4,
        "g" => 5,
        "z" => 6,
        "x" => 7,
        "c" => 8,
        "v" => 9,
        "b" => 11,
        "q" => 12,
        "w" => 13,
        "e" => 14,
        "r" => 15,
        "y" => 16,
        "t" => 17,
        "1" => 18,
        "2" => 19,
        "3" => 20,
        "4" => 21,
        "6" => 22,
        "5" => 23,
        "equal" | "=" => 24,
        "9" => 25,
        "7" => 26,
        "minus" | "-" => 27,
        "8" => 28,
        "0" => 29,
        "right_bracket" | "]" => 30,
        "o" => 31,
        "u" => 32,
        "left_bracket" | "[" => 33,
        "i" => 34,
        "p" => 35,
        "return_or_enter" | "return" | "enter" => 36,
        "l" => 37,
        "j" => 38,
        "quote" | "'" => 39,
        "k" => 40,
        "semicolon" | ";" => 41,
        "backslash" | "\\" => 42,
        "comma" | "," => 43,
        "slash" | "/" => 44,
        "n" => 45,
        "m" => 46,
        "period" | "." => 47,
        "tab" => 48,
        "spacebar" | "space" => 49,
        "grave_accent_and_tilde" | "`" => 50,
        "delete_or_backspace" | "backspace" | "delete" => 51,
        "escape" | "esc" => 53,
        "f1" => 122,
        "f2" => 120,
        "f3" => 99,
        "f4" => 118,
        "f5" => 96,
        "f6" => 97,
        "f7" => 98,
        "f8" => 100,
        "f9" => 101,
        "f10" => 109,
        "f11" => 103,
        "f12" => 111,
        "home" => 115,
        "page_up" => 116,
        "forward_delete" => 117,
        "end" => 119,
        "page_down" => 121,
        "left_arrow" => 123,
        "right_arrow" => 124,
        "down_arrow" => 125,
        "up_arrow" => 126,
        _ => return None,
    })
}

/// The flag bit for a modifier name. Either side of a modifier counts as the
/// same modifier, so `left_control` and `control` behave alike; which side a
/// synthetic event reports is decided when it is sent.
pub fn modifier_flag(name: &str) -> Option<u64> {
    Some(match normalized(name).as_str() {
        "shift" | "left_shift" | "right_shift" => SHIFT,
        "control" | "left_control" | "right_control" => CONTROL,
        "option" | "left_option" | "right_option" | "alt" => OPTION,
        "command" | "left_command" | "right_command" | "cmd" => COMMAND,
        _ => return None,
    })
}

fn normalized(name: &str) -> String {
    name.trim().to_lowercase().replace('-', "_")
}

/// The modifiers a keystroke is holding, ignoring caps lock, the numeric pad
/// and the function key, which no rule here can ask for.
fn held_modifiers(flags: u64) -> u64 {
    flags & (SHIFT | CONTROL | OPTION | COMMAND)
}

/// The device-dependent bits that make a synthetic modifier look like the
/// left-hand key, which is what every rule in the configuration came from.
const LEFT_HAND_BITS: &[(u64, u64)] = &[
    (CONTROL, 0x0000_0001),
    (SHIFT, 0x0000_0002),
    (COMMAND, 0x0000_0008),
    (OPTION, 0x0000_0020),
];

/// A rule with its names resolved once, so matching a keystroke stays a few
/// integer comparisons.
struct Resolved {
    from_key: i64,
    from_modifiers: u64,
    allow_extra_modifiers: bool,
    to_key: i64,
    to_modifiers: u64,
    except_apps: Vec<String>,
}

impl Resolved {
    fn new(remap: &KeyRemap) -> Option<Self> {
        let from_modifiers = remap
            .from_modifiers
            .iter()
            .filter_map(|modifier| modifier_flag(modifier))
            .fold(0, |flags, flag| flags | flag);
        let to_modifiers = remap
            .to_modifiers
            .iter()
            .filter_map(|modifier| modifier_flag(modifier))
            .fold(0, |flags, flag| flags | flag);
        Some(Self {
            from_key: key_code(&remap.from_key)?,
            from_modifiers,
            allow_extra_modifiers: remap.allow_extra_modifiers,
            to_key: key_code(&remap.to_key)?,
            to_modifiers,
            except_apps: remap.except_apps.clone(),
        })
    }

    fn matches(&self, key: i64, flags: u64, frontmost_app: Option<&str>) -> bool {
        if self.from_key != key {
            return false;
        }
        if let Some(app) = frontmost_app
            && self.except_apps.iter().any(|excluded| excluded == app)
        {
            return false;
        }
        let held = held_modifiers(flags);
        if self.allow_extra_modifiers {
            held & self.from_modifiers == self.from_modifiers
        } else {
            held == self.from_modifiers
        }
    }

    /// The flags an event for this rule carries: exactly the modifiers the rule
    /// asks for, plus the left-hand device bits so the combination behaves like
    /// a real key press everywhere.
    fn output_flags(&self) -> u64 {
        LEFT_HAND_BITS
            .iter()
            .filter(|(flag, _)| self.to_modifiers & flag != 0)
            .fold(self.to_modifiers, |flags, (_, bit)| flags | bit)
    }
}

/// Rewrites keystrokes according to the configured rules.
#[derive(Default)]
pub struct KeyRemapper {
    rules: Vec<Resolved>,
    /// Keys whose press was rewritten, so the release can be rewritten the same
    /// way even after the modifier that triggered the rule was let go.
    pressed: HashMap<i64, (i64, u64)>,
}

impl KeyRemapper {
    pub fn new(remaps: &[KeyRemap]) -> Self {
        Self {
            rules: remaps
                .iter()
                .filter(|remap| remap.enabled)
                .filter_map(Resolved::new)
                .collect(),
            pressed: HashMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The key code and flags an event should carry, or `None` to send it
    /// unchanged. A press is only rewritten when a rule matches it; the release
    /// and any repeat follow the press that was already rewritten.
    pub fn rewrite(
        &mut self,
        event_type: u32,
        key: i64,
        flags: u64,
        frontmost_app: Option<&str>,
    ) -> Option<(i64, u64)> {
        match event_type {
            KEY_DOWN => {
                if let Some(mapped) = self.pressed.get(&key) {
                    return Some(*mapped);
                }
                let mapped = self.rules.iter().find_map(|rule| {
                    rule.matches(key, flags, frontmost_app)
                        .then_some((rule.to_key, rule.output_flags()))
                })?;
                self.pressed.insert(key, mapped);
                Some(mapped)
            }
            KEY_UP => self.pressed.remove(&key),
            _ => None,
        }
    }

    /// Forget which keys are held. The event tap calls this when it is disabled
    /// or re-enabled, because a press that happened while it was off never
    /// reaches the release that would have cleared it.
    pub fn reset(&mut self) {
        self.pressed.clear();
    }
}
