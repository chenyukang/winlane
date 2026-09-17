use crate::{i18n::Locale, tr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandId {
    ShowMenu,
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

pub const COMMANDS: &[Command] = &[Command {
    id: CommandId::ShowMenu,
    name: "show-menu",
    title_en: "Show macOS menu bar",
    title_zh: "显示 macOS 菜单栏",
    symbol: "menubar.rectangle",
    keywords: &["show menu", "menu bar", "menubar", "显示菜单栏", "菜单栏"],
}];

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
