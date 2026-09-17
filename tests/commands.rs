use winlane::commands::{COMMANDS, CommandId, matching_commands};

#[test]
fn command_search_accepts_names_and_discoverable_prefixes() {
    for query in [
        "show-menu",
        "SHOW-MENU",
        " show- ",
        "show menu",
        "menu",
        "menubar",
        "菜单栏",
    ] {
        assert_eq!(matching_commands(query), [CommandId::ShowMenu], "{query}");
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
        for other in &COMMANDS[index + 1..] {
            assert_ne!(command.id, other.id);
            assert_ne!(command.name, other.name);
        }
    }
}
