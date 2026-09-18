use winlane::commands::{COMMANDS, CommandId, matching_commands};

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
