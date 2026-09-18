use crate::{i18n::Locale, tr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandId {
    Quicklinks,
    Snippets,
    Clipboard,
    ShowMenu,
    LockScreen,
    Sleep,
    MissionControl,
}

pub struct Command {
    pub id: CommandId,
    pub name: &'static str,
    pub title_en: &'static str,
    pub title_zh: &'static str,
    pub symbol: &'static str,
    pub keywords: &'static [&'static str],
}

impl Command {
    pub fn title(&self) -> &'static str {
        match crate::i18n::locale() {
            Locale::English => self.title_en,
            Locale::Chinese => self.title_zh,
        }
    }

    pub fn category(&self) -> &'static str {
        tr!("Winlane 命令", "Winlane command")
    }
}

pub const COMMANDS: &[Command] = &[
    Command {
        id: CommandId::Quicklinks,
        name: "quicklink",
        title_en: "Search quicklinks",
        title_zh: "搜索快捷链接",
        symbol: "link",
        keywords: &["quicklinks", "links", "快捷链接", "链接"],
    },
    Command {
        id: CommandId::Clipboard,
        name: "clipboard",
        title_en: "Search clipboard history",
        title_zh: "搜索剪贴板历史",
        symbol: "clipboard",
        keywords: &["clipboard history", "clip", "剪贴板", "剪贴板历史"],
    },
    Command {
        id: CommandId::Snippets,
        name: "snippet",
        title_en: "Search snippets",
        title_zh: "搜索文本片段",
        symbol: "text.quote",
        keywords: &["snippets", "text snippets", "片段", "文本片段"],
    },
    Command {
        id: CommandId::ShowMenu,
        name: "show-menu",
        title_en: "Show macOS menu bar",
        title_zh: "显示 macOS 菜单栏",
        symbol: "menubar.rectangle",
        keywords: &["show menu", "menu bar", "menubar", "显示菜单栏", "菜单栏"],
    },
    Command {
        id: CommandId::LockScreen,
        name: "lock-screen",
        title_en: "Lock screen",
        title_zh: "锁定屏幕",
        symbol: "lock",
        keywords: &["lock screen", "lock", "锁屏", "锁定屏幕"],
    },
    Command {
        id: CommandId::Sleep,
        name: "sleep",
        title_en: "Put Mac to sleep",
        title_zh: "让 Mac 进入睡眠",
        symbol: "moon.zzz",
        keywords: &["sleep mac", "睡眠", "休眠"],
    },
    Command {
        id: CommandId::MissionControl,
        name: "mission-control",
        title_en: "Open Mission Control",
        title_zh: "打开调度中心",
        symbol: "rectangle.3.group",
        keywords: &["mission control", "调度中心", "窗口总览"],
    },
];

impl CommandId {
    pub fn definition(self) -> &'static Command {
        COMMANDS.iter().find(|command| command.id == self).unwrap()
    }
}

pub fn matching_commands(query: &str) -> Vec<CommandId> {
    let query = normalize(query);
    if query.chars().count() < 2 {
        return Vec::new();
    }
    COMMANDS
        .iter()
        .filter(|command| {
            std::iter::once(command.name)
                .chain(command.keywords.iter().copied())
                .any(|keyword| normalize(keyword).starts_with(&query))
        })
        .map(|command| command.id)
        .collect()
}

fn normalize(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .replace('-', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
