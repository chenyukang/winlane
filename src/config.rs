use crate::search::{WindowInfo, rank};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use serde::{Deserialize, Serialize};

pub const KEYS: &[&str] = &[
    "Space",
    "Tab",
    "Backquote",
    "KeyA",
    "KeyB",
    "KeyC",
    "KeyD",
    "KeyE",
    "KeyF",
    "KeyG",
    "KeyH",
    "KeyI",
    "KeyJ",
    "KeyK",
    "KeyL",
    "KeyM",
    "KeyN",
    "KeyO",
    "KeyP",
    "KeyQ",
    "KeyR",
    "KeyS",
    "KeyT",
    "KeyU",
    "KeyV",
    "KeyW",
    "KeyX",
    "KeyY",
    "KeyZ",
    "F1",
    "F2",
    "F3",
    "F4",
    "F5",
    "F6",
    "F7",
    "F8",
    "F9",
    "F10",
    "F11",
    "F12",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shortcut {
    pub control: bool,
    pub option: bool,
    pub shift: bool,
    pub command: bool,
    pub key: String,
}

impl Default for Shortcut {
    fn default() -> Self {
        Self {
            control: true,
            option: true,
            shift: false,
            command: false,
            key: "Space".into(),
        }
    }
}

impl Shortcut {
    pub fn switch_default() -> Self {
        Self {
            control: false,
            option: false,
            shift: false,
            command: true,
            key: "Tab".into(),
        }
    }

    pub fn binding(&self) -> Result<crate::shortcuts::Binding, String> {
        use crate::shortcuts::{Binding, COMMAND, CONTROL, OPTION, SHIFT};
        self.hotkey()?;
        let key = match self.key.as_str() {
            "KeyA" => 0,
            "KeyS" => 1,
            "KeyD" => 2,
            "KeyF" => 3,
            "KeyH" => 4,
            "KeyG" => 5,
            "KeyZ" => 6,
            "KeyX" => 7,
            "KeyC" => 8,
            "KeyV" => 9,
            "KeyB" => 11,
            "KeyQ" => 12,
            "KeyW" => 13,
            "KeyE" => 14,
            "KeyR" => 15,
            "KeyY" => 16,
            "KeyT" => 17,
            "KeyO" => 31,
            "KeyU" => 32,
            "KeyI" => 34,
            "KeyP" => 35,
            "KeyL" => 37,
            "KeyJ" => 38,
            "KeyK" => 40,
            "KeyN" => 45,
            "KeyM" => 46,
            "Tab" => 48,
            "Space" => 49,
            "Backquote" => 50,
            "F1" => 122,
            "F2" => 120,
            "F3" => 99,
            "F4" => 118,
            "F5" => 96,
            "F6" => 97,
            "F7" => 98,
            "F8" => 100,
            "F9" => 101,
            "F10" => 109,
            "F11" => 103,
            "F12" => 111,
            _ => return Err("无法识别快捷键。".into()),
        };
        let mut modifiers = 0;
        for (enabled, flag) in [
            (self.control, CONTROL),
            (self.option, OPTION),
            (self.shift, SHIFT),
            (self.command, COMMAND),
        ] {
            if enabled {
                modifiers |= flag;
            }
        }
        Ok(Binding { key, modifiers })
    }

    pub fn release_label(&self) -> &str {
        if self.command {
            "⌘"
        } else if self.control {
            "⌃"
        } else {
            "⌥"
        }
    }

    pub fn is_command_tab(&self) -> bool {
        self.command && !self.control && !self.option && !self.shift && self.key == "Tab"
    }

    pub fn hotkey(&self) -> Result<HotKey, String> {
        let command_window_key =
            self.command && !self.shift && matches!(self.key.as_str(), "Tab" | "Backquote");
        if !self.control && !self.option && !command_window_key {
            return Err(
                "请选择 Command + Tab、Command + `，或包含 Control / Option 的组合。".into(),
            );
        }
        if !KEYS.contains(&self.key.as_str()) {
            return Err("不支持这个按键，请重新选择。".into());
        }
        let mut modifiers = Modifiers::empty();
        for (enabled, flag) in [
            (self.control, Modifiers::CONTROL),
            (self.option, Modifiers::ALT),
            (self.shift, Modifiers::SHIFT),
            (self.command, Modifiers::SUPER),
        ] {
            if enabled {
                modifiers |= flag;
            }
        }
        let code = self.key.parse::<Code>().map_err(|_| "无法识别快捷键。")?;
        Ok(HotKey::new(Some(modifiers), code))
    }

    pub fn display(&self) -> String {
        format!(
            "{}{}{}{}{}",
            if self.control { "⌃" } else { "" },
            if self.option { "⌥" } else { "" },
            if self.shift { "⇧" } else { "" },
            if self.command { "⌘" } else { "" },
            key_label(&self.key)
        )
    }
}

pub fn key_label(key: &str) -> &str {
    if key == "Backquote" {
        "`"
    } else {
        key.strip_prefix("Key").unwrap_or(key)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortOrder {
    #[default]
    Recent,
    Application,
    Title,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub shortcut: Shortcut,
    pub switch_shortcut: Shortcut,
    pub sort: SortOrder,
    pub appearance: Appearance,
    pub include_minimized: bool,
    pub excluded_apps: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shortcut: Shortcut::default(),
            switch_shortcut: Shortcut::switch_default(),
            sort: SortOrder::Recent,
            appearance: Appearance::System,
            include_minimized: true,
            excluded_apps: Vec::new(),
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        let search = self.shortcut.binding()?;
        let switch = self.switch_shortcut.binding()?;
        if self.switch_shortcut.key == "Space" {
            return Err("Space 用于切换模式，请为切换快捷键选择其他按键。".into());
        }
        if search.conflicts_with_switch(switch) {
            return Err("搜索和切换快捷键不能相同，也不能占用切换模式的 Shift 反向组合。".into());
        }
        if self.excluded_apps.len() > 100
            || self
                .excluded_apps
                .iter()
                .any(|app| app.chars().count() > 100)
        {
            return Err("最多排除 100 个应用，每个应用名不超过 100 个字符。".into());
        }
        Ok(())
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|_| "保存的设置无法读取，请在设置中重新保存。")?;
        let legacy = value.get("switch_shortcut").is_none();
        let mut config: Self = serde_json::from_value(value)
            .map_err(|_| "保存的设置无法读取，请在设置中重新保存。")?;
        if legacy
            && config
                .shortcut
                .binding()?
                .conflicts_with_switch(config.switch_shortcut.binding()?)
        {
            config.switch_shortcut = Shortcut {
                control: false,
                option: true,
                key: "Tab".into(),
                ..Shortcut::default()
            };
        }
        config.validate()?;
        Ok(config)
    }

    pub fn to_json(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string(self).map_err(|_| "无法保存设置。".into())
    }
}

pub fn parse_excluded(text: &str) -> Vec<String> {
    let mut apps = Vec::new();
    for name in text
        .split([',', '，', '\n'])
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        if !apps
            .iter()
            .any(|item: &String| item.to_lowercase() == name.to_lowercase())
        {
            apps.push(name.to_string());
        }
    }
    apps
}

pub fn visible_matches(
    windows: &[WindowInfo],
    query: &str,
    preferred: Option<u64>,
    config: &Config,
    scope_pid: Option<i32>,
    recent: &[u64],
    previous_pid: i32,
) -> Vec<usize> {
    let excluded: Vec<_> = config
        .excluded_apps
        .iter()
        .map(|name| name.trim().to_lowercase())
        .collect();
    let mut order: Vec<_> = (0..windows.len())
        .filter(|&i| {
            let window = &windows[i];
            (config.include_minimized || !window.minimized)
                && scope_pid.is_none_or(|pid| pid == window.pid)
                && !excluded.contains(&window.app.to_lowercase())
        })
        .collect();
    match config.sort {
        SortOrder::Recent => order.sort_by_key(|&i| {
            let w = &windows[i];
            (
                recent
                    .iter()
                    .position(|id| *id == w.id)
                    .unwrap_or(usize::MAX),
                w.pid != previous_pid,
                w.minimized,
            )
        }),
        SortOrder::Application => order.sort_by_key(|&i| {
            (
                windows[i].app.to_lowercase(),
                windows[i].title.to_lowercase(),
            )
        }),
        SortOrder::Title => order.sort_by_key(|&i| {
            (
                windows[i].title.to_lowercase(),
                windows[i].app.to_lowercase(),
            )
        }),
    }
    let candidates: Vec<_> = order.iter().map(|&i| windows[i].clone()).collect();
    rank(&candidates, query, preferred)
        .into_iter()
        .map(|index| order[index])
        .collect()
}
