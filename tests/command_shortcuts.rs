use winlane::core::commands::{COMMANDS, CommandId};
use winlane::core::config::{AppShortcut, ApplicationTarget, CommandShortcut, Config, Shortcut};
use winlane::core::shortcuts::*;

fn shortcut(key: &str) -> Shortcut {
    Shortcut {
        command: true,
        option: true,
        control: false,
        shift: false,
        key: key.into(),
    }
}

fn configured() -> Config {
    Config {
        command_shortcuts: vec![CommandShortcut {
            command: CommandId::OpenUrl,
            shortcut: shortcut("KeyU"),
        }],
        ..Config::default()
    }
}

fn router(config: &Config) -> ShortcutRouter {
    ShortcutRouter::new(
        config.shortcut.binding().unwrap(),
        config.switch_shortcut.binding().unwrap(),
    )
    .with_command_shortcuts(config.command_bindings().unwrap())
}

#[test]
fn command_bindings_roundtrip_without_changing_existing_settings() {
    let empty = Config::from_json("{}").unwrap();
    assert!(empty.command_shortcuts.is_empty());
    assert!(
        serde_json::to_value(&empty)
            .unwrap()
            .get("command_shortcuts")
            .is_none()
    );
    let config = configured();
    assert_eq!(
        Config::from_json(&config.to_json().unwrap()).unwrap(),
        config
    );
    for command in COMMANDS {
        assert_eq!(serde_json::to_value(command.id).unwrap(), command.name);
        assert_eq!(
            serde_json::from_value::<CommandId>(serde_json::json!(command.name)).unwrap(),
            command.id
        );
    }
    assert!(serde_json::from_str::<CommandId>("\"recent-url\"").is_err());
}

#[test]
fn command_bindings_reject_conflicts_and_duplicate_commands() {
    let mut config = configured();
    config.additional_search_shortcuts = vec![shortcut("KeyF")];
    config.app_shortcuts = vec![AppShortcut {
        shortcut: shortcut("KeyA"),
        application: ApplicationTarget {
            name: "Example".into(),
            bundle_id: "com.example.editor".into(),
            path: "/Applications/Example.app".into(),
        },
    }];
    config.quicklinks = vec![winlane::features::quicklinks::Quicklink {
        id: "example".into(),
        name: "Example".into(),
        link: "https://example.com".into(),
        open_with: String::new(),
        shortcut: Some(shortcut("KeyL")),
    }];
    config.validate().unwrap();
    let mut reverse = config.switch_shortcut.clone();
    reverse.shift = true;
    for conflict in [
        config.shortcut.clone(),
        config.additional_search_shortcuts[0].clone(),
        config.switch_shortcut.clone(),
        reverse,
        config.app_shortcuts[0].shortcut.clone(),
        shortcut("KeyL"),
    ] {
        config.command_shortcuts[0].shortcut = conflict;
        assert!(config.validate().is_err());
    }
    config.command_shortcuts[0].shortcut = shortcut("KeyU");
    config.command_shortcuts.push(CommandShortcut {
        command: CommandId::Projects,
        shortcut: shortcut("KeyU"),
    });
    assert!(config.validate().is_err());
    config.command_shortcuts[1].shortcut = shortcut("KeyP");
    config.validate().unwrap();
    config.command_shortcuts[1].command = CommandId::OpenUrl;
    assert!(config.validate().is_err());
    config.command_shortcuts.pop();
    config.command_shortcuts[0].shortcut.command = false;
    config.command_shortcuts[0].shortcut.option = false;
    assert!(config.validate().is_err());
}

#[test]
fn commands_fire_once_and_cancel_pending_switch_acceptance() {
    for command in COMMANDS {
        let mut config = configured();
        config.command_shortcuts[0].command = command.id;
        let key = config.command_bindings().unwrap()[0].1;
        let mut router = router(&config);
        let old = router.enter_switch(COMMAND);
        let (consume, action) = router.handle(KEY_DOWN, key.key, key.modifiers, false);
        assert!(consume);
        let action = action.unwrap();
        assert_eq!(action.kind, ActionKind::RunCommand(command.id));
        assert_ne!(action.session, old.session);
        assert_eq!(
            router.handle(KEY_DOWN, key.key, key.modifiers, true),
            (true, None)
        );
        assert_eq!(router.handle(KEY_UP, key.key, 0, false), (true, None));
        assert_eq!(router.handle(FLAGS_CHANGED, 0, 0, false), (false, None));
        router.resume_search(action.session);
        let search = config.shortcut.binding().unwrap();
        let (_, cancel) = router.handle(KEY_DOWN, search.key, search.modifiers, false);
        assert_eq!(cancel.unwrap().kind, ActionKind::Cancel);
    }
}

#[test]
fn rebinding_or_clearing_a_command_removes_the_old_trigger() {
    let mut config = configured();
    let old = config.command_bindings().unwrap()[0].1;
    config.command_shortcuts[0].shortcut = shortcut("KeyO");
    let new = config.command_bindings().unwrap()[0].1;
    let mut changed = router(&config);
    assert_eq!(
        changed.handle(KEY_DOWN, old.key, old.modifiers, false),
        (false, None)
    );
    assert_eq!(
        changed
            .handle(KEY_DOWN, new.key, new.modifiers, false)
            .1
            .unwrap()
            .kind,
        ActionKind::RunCommand(CommandId::OpenUrl)
    );
    config.command_shortcuts.clear();
    assert_eq!(
        router(&config).handle(KEY_DOWN, new.key, new.modifiers, false),
        (false, None)
    );
}
