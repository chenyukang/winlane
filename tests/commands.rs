use winlane::core::commands::{COMMANDS, CommandId, matching_commands};

#[test]
fn emoji_command_has_an_explicit_search_entry() {
    for query in ["emoji", "emojis", " EMOJI ", "emo", "表情", "表情符号"] {
        assert_eq!(matching_commands(query), [CommandId::Emoji]);
    }
    for query in ["e", "😀", "rocket"] {
        assert!(!matching_commands(query).contains(&CommandId::Emoji));
    }
    assert_eq!(
        serde_json::to_string(&CommandId::Emoji).unwrap(),
        "\"emoji\""
    );
}

#[test]
fn date_command_has_an_explicit_search_entry() {
    for query in [
        "date", "DATE", " date ", "time", "clock", "datetime", "now", "日期", "时间", "现在",
        "几点",
    ] {
        assert_eq!(matching_commands(query), [CommandId::Date], "{query}");
    }
    for query in ["d", "co", "rust"] {
        assert!(
            !matching_commands(query).contains(&CommandId::Date),
            "{query}"
        );
    }
    assert_eq!(serde_json::to_string(&CommandId::Date).unwrap(), "\"date\"");
}

#[test]
fn branch_command_has_an_explicit_search_entry() {
    for query in [
        "branch",
        "BRANCH",
        " branch ",
        "git",
        "git branch",
        "checkout",
        "分支",
        "切换分支",
    ] {
        assert_eq!(matching_commands(query), [CommandId::GitBranch], "{query}");
    }
    for query in ["b", "g", "switch branch extra", "commit", "vscode"] {
        assert!(
            !matching_commands(query).contains(&CommandId::GitBranch),
            "{query}"
        );
    }
    assert_eq!(
        serde_json::to_string(&CommandId::GitBranch).unwrap(),
        "\"branch\""
    );
}

#[test]
fn appearance_toggle_matches_both_modes_and_languages() {
    for query in [
        "toggle-appearance",
        "toggle appearance",
        " TOGGLE DARK MODE ",
        "toggle light/dark mode",
        "toggle light mode",
        "dark mode",
        "light mode",
        "dark",
        "light",
        "theme",
        "appearance",
        "切换外观",
        "切换深浅模式",
        "深色模式",
        "浅色模式",
        "暗黑模式",
    ] {
        assert_eq!(
            matching_commands(query),
            [CommandId::ToggleAppearance],
            "{query}"
        );
    }
    for query in ["t", "d", "l", "toggle-appearance extra", "dark mode; sleep"] {
        assert!(matching_commands(query).is_empty(), "{query}");
    }
}

#[test]
fn screenshot_matches_area_capture_keywords() {
    for query in [
        "screenshot",
        "screen shot",
        "capture area",
        "screen",
        "截图",
        "截屏",
        "区域截图",
    ] {
        assert_eq!(matching_commands(query), [CommandId::Screenshot], "{query}");
    }
}

#[test]
fn command_search_accepts_names_and_discoverable_prefixes() {
    for query in [
        "show-menu",
        "SHOW-MENU",
        " show-m ",
        "show menu",
        "menu",
        "menubar",
        "菜单栏",
    ] {
        assert_eq!(matching_commands(query), [CommandId::ShowMenu], "{query}");
    }
}

#[test]
fn system_commands_match_english_names_and_chinese_keywords() {
    for (command, queries) in [
        (
            CommandId::LockScreen,
            ["lock-screen", "LOCK SCREEN", "lock", "锁屏", "锁定屏幕"],
        ),
        (
            CommandId::Sleep,
            ["sleep", " SLEEP ", "sleep mac", "睡眠", "休眠"],
        ),
        (
            CommandId::MissionControl,
            [
                "mission-control",
                "MISSION CONTROL",
                "mission",
                "调度中心",
                "窗口总览",
            ],
        ),
    ] {
        for query in queries {
            assert_eq!(matching_commands(query), [command], "{query}");
        }
    }
    for query in ["show", " show- "] {
        assert_eq!(matching_commands(query), [CommandId::ShowMenu]);
    }
}

#[test]
fn commands_do_not_pollute_empty_search_aliases_or_unrelated_queries() {
    for query in [
        "",
        " ",
        "s",
        "m",
        "co",
        "rust",
        "show-menu extra",
        "show-menu; echo hi",
        "lock-screen; sleep",
        "sleep now please",
        "desktop extra",
        "show-desktop",
        "desktop",
        "显示桌面",
        "mission control extra",
        "l",
    ] {
        assert!(matching_commands(query).is_empty(), "{query}");
    }
}

#[test]
fn registered_commands_have_unique_names_and_ids() {
    for (index, command) in COMMANDS.iter().enumerate() {
        assert_eq!(command.id.definition().name, command.name);
        assert!(!command.title_en.is_empty());
        assert!(!command.title_zh.is_empty());
        assert_eq!(matching_commands(command.name), [command.id]);
        for other in &COMMANDS[index + 1..] {
            assert_ne!(command.id, other.id);
            assert_ne!(command.name, other.name);
        }
    }
}

#[test]
fn snippet_command_has_an_explicit_search_entry() {
    for query in [
        "snippet",
        "snippets",
        " SNIPPET ",
        "snip",
        "text snippets",
        "片段",
        "文本片段",
    ] {
        assert_eq!(matching_commands(query), [CommandId::Snippets]);
    }
    for query in ["r", "rust", "greeting"] {
        assert!(matching_commands(query).is_empty());
    }
}

#[test]
fn clipboard_command_has_an_explicit_search_entry() {
    for query in [
        "clipboard",
        "clipboard history",
        "CLIP",
        "剪贴板",
        "剪贴板历史",
    ] {
        assert_eq!(matching_commands(query), [CommandId::Clipboard]);
    }
    for query in ["c", "co", "copied contents", "first test copy"] {
        assert!(matching_commands(query).is_empty());
    }
}

#[test]
fn quicklinks_have_an_explicit_search_entry() {
    for query in [
        "quicklink",
        "quicklinks",
        "Quick",
        "links",
        "快捷链接",
        "链接",
    ] {
        assert_eq!(matching_commands(query), [CommandId::Quicklinks]);
    }
}

#[test]
fn projects_have_an_explicit_entry() {
    for query in [
        "projects",
        "project",
        "vscode",
        "vs code",
        "RECENT PROJECTS",
        "最近项目",
        "项目",
    ] {
        assert_eq!(matching_commands(query), [CommandId::Projects]);
    }
    assert!(matching_commands("p").is_empty());
}
