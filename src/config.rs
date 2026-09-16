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
    pub fn is_command_tab(&self) -> bool {
        self.command && !self.control && !self.option && !self.shift && self.key == "Tab"
    }

    pub fn hotkey(&self) -> Result<HotKey, String> {
        if !self.control && !self.option && !self.is_command_tab() {
            return Err("请选择 Command + Tab，或包含 Control / Option 的组合。".into());
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
    pub sort: SortOrder,
    pub appearance: Appearance,
    pub include_minimized: bool,
    pub excluded_apps: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shortcut: Shortcut::default(),
            sort: SortOrder::Recent,
            appearance: Appearance::System,
            include_minimized: true,
            excluded_apps: Vec::new(),
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        self.shortcut.hotkey()?;
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
        let config: Self =
            serde_json::from_str(json).map_err(|_| "保存的设置无法读取，请在设置中重新保存。")?;
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
