use crate::core::i18n::Locale;
use crate::tr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommandId {
    Files,
    KeepAwake,
    Bluetooth,
    Projects,
    OpenUrl,
    #[serde(rename = "quicklink")]
    Quicklinks,
    #[serde(rename = "snippet")]
    Snippets,
    Clipboard,
    ShowMenu,
    LockScreen,
    Screenshot,
    ToggleAppearance,
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
        match crate::core::i18n::locale() {
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
        id: CommandId::Files,
        name: "files",
        title_en: "Search files and folders",
        title_zh: "查找文件和文件夹",
        symbol: "doc.text.magnifyingglass",
        keywords: &["file", "finder", "documents", "文件", "文件夹"],
    },
    Command {
        id: CommandId::KeepAwake,
        name: "keep-awake",
        title_en: "Keep Mac awake",
        title_zh: "保持 Mac 唤醒",
        symbol: "cup.and.saucer",
        keywords: &["caffeine", "awake", "prevent sleep", "防休眠", "保持唤醒"],
    },
    Command {
        id: CommandId::Bluetooth,
        name: "bluetooth",
        title_en: "Connect or disconnect Bluetooth devices",
        title_zh: "连接或断开蓝牙设备",
        symbol: "antenna.radiowaves.left.and.right",
        keywords: &["bt", "蓝牙", "蓝牙设备"],
    },
    Command {
        id: CommandId::OpenUrl,
        name: "open-url",
        title_en: "Open URL or search Google",
        title_zh: "打开网址或 Google 搜索",
        symbol: "clock.arrow.circlepath",
        keywords: &[
            "open url",
            "打开网址",
            "history",
            "最近网址",
            "浏览记录",
            "历史网址",
        ],
    },
    Command {
        id: CommandId::Projects,
        name: "projects",
        title_en: "Open recent VS Code projects",
        title_zh: "打开 VS Code 最近项目",
        symbol: "folder",
        keywords: &[
            "project",
            "vscode",
            "vs code",
            "recent projects",
            "最近项目",
            "项目",
        ],
    },
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
        id: CommandId::Screenshot,
        name: "screenshot",
        title_en: "Capture an area to clipboard",
        title_zh: "框选截图并复制到剪贴板",
        symbol: "viewfinder",
        keywords: &[
            "screen shot",
            "capture area",
            "screenshot area",
            "截图",
            "截屏",
            "区域截图",
        ],
    },
    Command {
        id: CommandId::ToggleAppearance,
        name: "toggle-appearance",
        title_en: "Toggle system light / dark mode",
        title_zh: "切换系统浅色 / 深色模式",
        symbol: "circle.lefthalf.filled",
        keywords: &[
            "toggle light/dark mode",
            "toggle light dark mode",
            "toggle dark mode",
            "toggle light mode",
            "dark mode",
            "light mode",
            "appearance",
            "theme",
            "切换外观",
            "切换深浅模式",
            "深色模式",
            "浅色模式",
            "暗黑模式",
        ],
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
