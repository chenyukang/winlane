use crate::core::i18n::Language;
use crate::core::search::{Query, WindowInfo, rank};
use crate::{tr, trf};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use serde::{Deserialize, Serialize};

pub const KEYS: &[&str] = &[
    "Space",
    "Tab",
    "Backquote",
    "Digit1",
    "Digit2",
    "Digit3",
    "Digit4",
    "Digit5",
    "Digit6",
    "Digit7",
    "Digit8",
    "Digit9",
    "Digit0",
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

pub const MAX_SEARCH_SHORTCUTS: usize = 8;

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
            option: false,
            shift: false,
            command: false,
            key: "KeyI".into(),
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

    pub fn binding(&self) -> Result<crate::core::shortcuts::Binding, String> {
        self.hotkey()?;
        self.app_binding()
    }

    pub fn app_binding(&self) -> Result<crate::core::shortcuts::Binding, String> {
        use crate::core::shortcuts::{Binding, COMMAND, CONTROL, OPTION, SHIFT};
        if !self.command && !self.control && !self.option {
            return Err(tr!(
                "快捷键需要包含 Command、Control 或 Option。",
                "Shortcuts must include Command, Control, or Option."
            )
            .into());
        }
        let key = match self.key.as_str() {
            "Digit1" => 18,
            "Digit2" => 19,
            "Digit3" => 20,
            "Digit4" => 21,
            "Digit5" => 23,
            "Digit6" => 22,
            "Digit7" => 26,
            "Digit8" => 28,
            "Digit9" => 25,
            "Digit0" => 29,
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
            _ => return Err(tr!("无法识别快捷键。", "The shortcut key is not recognized.").into()),
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
        let command_panel_key = self.command
            && !self.shift
            && matches!(self.key.as_str(), "Space" | "Tab" | "Backquote");
        if !self.control && !self.option && !command_panel_key {
            return Err(tr!(
                "请选择 Command + Space、Command + Tab、Command + `，或包含 Control / Option 的组合。",
                "Choose Command + Space, Command + Tab, Command + `, or a shortcut with Control / Option."
            )
            .into());
        }
        if !KEYS.contains(&self.key.as_str()) {
            return Err(tr!(
                "不支持这个按键，请重新选择。",
                "This key is not supported. Choose another key."
            )
            .into());
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
        let code = self
            .key
            .parse::<Code>()
            .map_err(|_| tr!("无法识别快捷键。", "The shortcut key is not recognized."))?;
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
        key.strip_prefix("Key")
            .or_else(|| key.strip_prefix("Digit"))
            .unwrap_or(key)
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayDensity {
    Compact,
    #[default]
    Normal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationTarget {
    pub bundle_id: String,
    pub path: String,
    pub name: String,
}

impl ApplicationTarget {
    pub fn validate(&self) -> Result<(), String> {
        let path = std::path::Path::new(&self.path);
        if self.bundle_id.trim().is_empty()
            || self.name.trim().is_empty()
            || self.bundle_id.contains('\0')
            || self.path.contains('\0')
            || !path.is_absolute()
            || path.extension().is_none_or(|extension| extension != "app")
        {
            return Err(tr!(
                "请选择一个有效的 .app 应用。",
                "Choose a valid .app application."
            )
            .into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppShortcut {
    pub shortcut: Shortcut,
    pub application: ApplicationTarget,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandShortcut {
    pub command: crate::core::commands::CommandId,
    pub shortcut: Shortcut,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasRule {
    pub alias: String,
    pub application: ApplicationTarget,
    #[serde(default)]
    pub title_contains: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub shortcut: Shortcut,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_search_shortcuts: Vec<Shortcut>,
    pub switch_shortcut: Shortcut,
    pub app_shortcuts: Vec<AppShortcut>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command_shortcuts: Vec<CommandShortcut>,
    pub alias_rules: Vec<AliasRule>,
    pub snippets: Vec<crate::features::snippets::Snippet>,
    pub quicklinks: Vec<crate::features::quicklinks::Quicklink>,
    pub clipboard: crate::features::clipboard::ClipboardSettings,
    pub sort: SortOrder,
    pub appearance: Appearance,
    pub display_density: DisplayDensity,
    pub background_opacity: u8,
    pub show_usage_hints: bool,
    pub switch_delay_ms: u16,
    pub language: Language,
    pub input_method: crate::core::input_method::InputMethod,
    pub input_indicator: crate::features::input_indicator::Settings,
    pub include_minimized: bool,
    pub excluded_apps: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shortcut: Shortcut::default(),
            additional_search_shortcuts: Vec::new(),
            switch_shortcut: Shortcut::switch_default(),
            app_shortcuts: Vec::new(),
            command_shortcuts: Vec::new(),
            alias_rules: Vec::new(),
            snippets: Vec::new(),
            quicklinks: Vec::new(),
            clipboard: Default::default(),
            sort: SortOrder::Recent,
            appearance: Appearance::System,
            display_density: DisplayDensity::default(),
            background_opacity: 100,
            show_usage_hints: true,
            switch_delay_ms: 100,
            language: Language::System,
            input_method: crate::core::input_method::InputMethod::default(),
            input_indicator: Default::default(),
            include_minimized: true,
            excluded_apps: Vec::new(),
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        crate::features::snippets::validate(&self.snippets)?;
        crate::features::quicklinks::validate(&self.quicklinks)?;
        self.clipboard.validate()?;
        self.input_indicator.validate()?;
        if self.alias_rules.len() > 64 {
            return Err(tr!(
                "最多设置 64 条 alias 规则。",
                "You can configure up to 64 alias rules."
            )
            .into());
        }
        let mut aliases = std::collections::HashSet::new();
        let mut targets = std::collections::HashSet::new();
        for rule in &self.alias_rules {
            rule.application.validate()?;
            if !(1..=2).contains(&rule.alias.len())
                || !rule.alias.bytes().all(|ch| ch.is_ascii_lowercase())
            {
                return Err(tr!(
                    "Alias 必须是 1–2 个小写英文字母。",
                    "An alias must contain 1–2 lowercase English letters."
                )
                .into());
            }
            if !aliases.insert(&rule.alias) {
                return Err(trf!(
                    "Alias {} 重复。",
                    "Alias {} is duplicated.",
                    rule.alias
                ));
            }
            if rule.title_contains.chars().count() > 200
                || rule.title_contains.trim() != rule.title_contains
            {
                return Err(tr!(
                    "标题关键词不能超过 200 个字符，且不能以空格开头或结尾。",
                    "Title keywords must be at most 200 characters, without surrounding spaces."
                )
                .into());
            }
            if !targets.insert((
                &rule.application.bundle_id,
                rule.title_contains.to_lowercase(),
            )) {
                return Err(tr!(
                    "同一应用和标题关键词只能设置一条规则。",
                    "Only one rule is allowed for the same app and title keywords."
                )
                .into());
            }
        }
        if self.background_opacity > 100 {
            return Err(tr!(
                "背景不透明度必须在 0–100% 之间。",
                "Background opacity must be between 0 and 100%."
            )
            .into());
        }
        if self.switch_delay_ms > 1000 {
            return Err(tr!(
                "显示延迟应在 0–1000 毫秒之间。",
                "Display delay must be between 0 and 1000 ms."
            )
            .into());
        }
        if self.additional_search_shortcuts.len() >= MAX_SEARCH_SHORTCUTS {
            return Err(tr!(
                "最多设置 8 个搜索快捷键。",
                "You can configure up to 8 search shortcuts."
            )
            .into());
        }
        let searches = self.search_bindings()?;
        let switch = self.switch_shortcut.binding()?;
        if self.switch_shortcut.key == "Space" {
            return Err(tr!(
                "Space 用于切换模式，请为切换快捷键选择其他按键。",
                "Space changes modes. Choose another key for the switch shortcut."
            )
            .into());
        }
        let mut assigned = Vec::new();
        let mut assign =
            |shortcut: &Shortcut, binding: crate::core::shortcuts::Binding, owner: String| {
                if let Some((_, existing)) = assigned.iter().find(|(key, _)| *key == binding) {
                    return Err(trf!(
                        "快捷键 {} 冲突：{} 与 {}。请选择其他组合。",
                        "Shortcut {} conflicts: {} and {}. Choose another combination.",
                        shortcut.display(),
                        existing,
                        owner
                    ));
                }
                assigned.push((binding, owner));
                Ok(())
            };
        assign(
            &self.switch_shortcut,
            switch,
            tr!("切换模式", "Switch mode").into(),
        )?;
        if !self.switch_shortcut.shift {
            let reverse = Shortcut {
                shift: true,
                ..self.switch_shortcut.clone()
            };
            assign(
                &reverse,
                reverse.app_binding()?,
                tr!("切换模式（反向）", "Switch mode (reverse)").into(),
            )?;
        }
        for (index, (shortcut, binding)) in std::iter::once(&self.shortcut)
            .chain(&self.additional_search_shortcuts)
            .zip(searches)
            .enumerate()
        {
            assign(
                shortcut,
                binding,
                trf!(
                    "搜索模式（快捷键 {}）",
                    "Search mode (shortcut {})",
                    index + 1
                ),
            )?;
        }
        if self.app_shortcuts.len() > 32 {
            return Err(tr!(
                "最多设置 32 个应用快捷键。",
                "You can configure up to 32 app shortcuts."
            )
            .into());
        }
        for item in &self.app_shortcuts {
            item.application.validate()?;
            assign(
                &item.shortcut,
                item.shortcut.app_binding()?,
                trf!("应用“{}”", "App “{}”", item.application.name),
            )?;
        }
        for link in &self.quicklinks {
            let Some(shortcut) = &link.shortcut else {
                continue;
            };
            assign(
                shortcut,
                shortcut.app_binding()?,
                trf!("快捷链接“{}”", "Quicklink “{}”", link.name),
            )?;
        }
        let mut commands = Vec::new();
        for item in &self.command_shortcuts {
            let name = item.command.definition().name;
            if commands.contains(&item.command) {
                return Err(trf!(
                    "命令“{}”只能设置一个快捷键。",
                    "Command “{}” can have only one shortcut.",
                    name
                ));
            }
            assign(
                &item.shortcut,
                item.shortcut.app_binding()?,
                trf!("命令“{}”", "Command “{}”", name),
            )?;
            commands.push(item.command);
        }
        if self.excluded_apps.len() > 100
            || self
                .excluded_apps
                .iter()
                .any(|app| app.chars().count() > 100)
        {
            return Err(tr!(
                "最多排除 100 个应用，每个应用名不超过 100 个字符。",
                "Exclude up to 100 apps, with at most 100 characters per name."
            )
            .into());
        }
        Ok(())
    }

    pub fn app_bindings(&self) -> Result<Vec<crate::core::shortcuts::Binding>, String> {
        self.app_shortcuts
            .iter()
            .map(|item| item.shortcut.app_binding())
            .collect()
    }

    pub fn command_bindings(
        &self,
    ) -> Result<
        Vec<(
            crate::core::commands::CommandId,
            crate::core::shortcuts::Binding,
        )>,
        String,
    > {
        self.command_shortcuts
            .iter()
            .map(|item| {
                item.shortcut
                    .app_binding()
                    .map(|binding| (item.command, binding))
            })
            .collect()
    }

    pub fn quicklink_bindings(
        &self,
    ) -> Result<Vec<(String, crate::core::shortcuts::Binding)>, String> {
        self.quicklinks
            .iter()
            .filter_map(|link| {
                link.shortcut.as_ref().map(|shortcut| {
                    shortcut
                        .app_binding()
                        .map(|binding| (link.id.clone(), binding))
                })
            })
            .collect()
    }

    pub fn search_bindings(&self) -> Result<Vec<crate::core::shortcuts::Binding>, String> {
        std::iter::once(&self.shortcut)
            .chain(&self.additional_search_shortcuts)
            .map(Shortcut::binding)
            .collect()
    }

    pub fn search_shortcuts_display(&self) -> String {
        std::iter::once(&self.shortcut)
            .chain(&self.additional_search_shortcuts)
            .map(Shortcut::display)
            .collect::<Vec<_>>()
            .join(" / ")
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(json).map_err(|_| {
            tr!(
                "保存的设置无法读取，请在设置中重新保存。",
                "Saved settings could not be read. Save them again in Settings."
            )
        })?;
        let legacy = value.get("switch_shortcut").is_none();
        let mut config: Self = serde_json::from_value(value).map_err(|_| {
            tr!(
                "保存的设置无法读取，请在设置中重新保存。",
                "Saved settings could not be read. Save them again in Settings."
            )
        })?;
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
        serde_json::to_string(self)
            .map_err(|_| tr!("无法保存设置。", "Settings could not be saved.").into())
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
    if config.sort == SortOrder::Recent {
        let Some(query) = Query::new(query) else {
            return Vec::new();
        };
        if !query.is_empty() {
            let mut matches: Vec<_> = order
                .into_iter()
                .filter_map(|i| {
                    let (priority, quality) = query.window_match(&windows[i])?;
                    Some((i, priority, std::cmp::Reverse(quality)))
                })
                .collect();
            matches.sort_by_key(|&(_, priority, quality)| (priority, quality));
            return matches.into_iter().map(|(i, _, _)| i).collect();
        }
        return order;
    }
    let candidates: Vec<_> = order.iter().map(|&i| windows[i].clone()).collect();
    rank(&candidates, query, preferred)
        .into_iter()
        .map(|index| order[index])
        .collect()
}
