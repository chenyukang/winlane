use winlane::core::config::{AppShortcut, ApplicationTarget, Config, Shortcut, key_label};
use winlane::core::shortcuts::*;

fn shortcut(key: &str) -> Shortcut {
    Shortcut {
        command: true,
        control: false,
        option: false,
        shift: false,
        key: key.into(),
    }
}

fn item(key: &str) -> AppShortcut {
    AppShortcut {
        shortcut: shortcut(key),
        application: ApplicationTarget {
            bundle_id: "com.example.browser".into(),
            path: "/Applications/Example Browser.app".into(),
            name: "Example Browser".into(),
        },
    }
}

#[test]
fn app_bindings_persist_without_changing_existing_settings() {
    let old = Config::from_json(r#"{"sort":"Title","excluded_apps":["Finder"]}"#).unwrap();
    assert!(old.app_shortcuts.is_empty());
    let configured = Config {
        app_shortcuts: vec![item("Digit1"), item("Digit2")],
        ..old.clone()
    };
    assert_eq!(
        Config::from_json(&configured.to_json().unwrap()).unwrap(),
        configured
    );
    assert_eq!(configured.shortcut, old.shortcut);
    assert_eq!(configured.switch_shortcut, old.switch_shortcut);
    assert_eq!(configured.sort, old.sort);
}

#[test]
fn app_bindings_reject_duplicates_and_search_or_reverse_switch_conflicts() {
    let mut config = Config {
        app_shortcuts: vec![item("Digit1"), item("Digit1")],
        ..Config::default()
    };
    assert!(config.validate().is_err());
    config.app_shortcuts.truncate(1);
    config.app_shortcuts[0].shortcut = config.shortcut.clone();
    assert!(config.validate().is_err());
    config.app_shortcuts[0].shortcut = config.switch_shortcut.clone();
    assert!(config.validate().is_err());
    config.app_shortcuts[0].shortcut.shift = true;
    assert!(config.validate().is_err());
    config.app_shortcuts[0].shortcut = shortcut("Digit1");
    config.app_shortcuts[0].shortcut.shift = true;
    assert!(config.validate().is_ok());
}

#[test]
fn app_targets_and_shortcuts_are_validated_before_saving() {
    for path in [
        "relative.app",
        "https://example.test/Example.app",
        "/Applications/document.txt",
        "/Applications/Bad\0.app",
    ] {
        let mut config = Config {
            app_shortcuts: vec![item("Digit1")],
            ..Config::default()
        };
        config.app_shortcuts[0].application.path = path.into();
        assert!(config.to_json().is_err(), "{path:?}");
    }
    let mut plain = shortcut("KeyA");
    plain.command = false;
    assert!(plain.app_binding().is_err());
    plain.shift = true;
    assert!(plain.app_binding().is_err());
    assert!(shortcut("KeyQ").app_binding().is_ok());
    assert!(
        shortcut("KeyQ").binding().is_err(),
        "search/switch restrictions stay unchanged"
    );
    assert!(shortcut("Invalid").app_binding().is_err());
    for (key, code) in [
        ("Digit1", 18),
        ("Digit2", 19),
        ("Digit5", 23),
        ("Digit6", 22),
        ("Digit0", 29),
    ] {
        assert_eq!(
            shortcut(key).app_binding().unwrap(),
            Binding {
                key: code,
                modifiers: COMMAND
            }
        );
        assert_eq!(key_label(key), &key[5..]);
    }
}

fn router() -> ShortcutRouter {
    let config = Config {
        app_shortcuts: vec![item("Digit1"), item("Digit2"), item("KeyW")],
        ..Config::default()
    };
    ShortcutRouter::new(
        config.shortcut.binding().unwrap(),
        config.switch_shortcut.binding().unwrap(),
    )
    .with_app_shortcuts(config.app_bindings().unwrap())
}

#[test]
fn fixed_shortcuts_trigger_once_immediately_and_consume_both_key_events() {
    let mut router = router();
    let (consume, action) = router.handle(KEY_DOWN, 18, COMMAND, false);
    assert!(consume);
    assert_eq!(action.unwrap().kind, ActionKind::LaunchApp(0));
    assert_eq!(router.handle(KEY_DOWN, 18, COMMAND, true), (true, None));
    assert_eq!(router.handle(KEY_DOWN, 18, COMMAND, false), (true, None));
    assert_eq!(router.handle(FLAGS_CHANGED, 55, 0, false), (false, None));
    assert_eq!(router.handle(KEY_UP, 18, 0, false), (true, None));
    assert_eq!(
        router.handle(KEY_DOWN, 19, COMMAND, false).1.unwrap().kind,
        ActionKind::LaunchApp(1)
    );
    assert_eq!(router.handle(KEY_UP, 19, 0, false), (true, None));
    assert_eq!(
        router.handle(KEY_DOWN, 18, COMMAND | SHIFT, false),
        (false, None)
    );
    assert_eq!(router.handle(KEY_DOWN, 20, COMMAND, false), (false, None));
    assert_eq!(router.handle(KEY_DOWN, 18, 0, false), (false, None));
}

#[test]
fn fixed_binding_leaves_switching_and_prevents_a_second_commit_on_release() {
    let mut router = router();
    let switch = router.handle(KEY_DOWN, TAB, COMMAND, false).1.unwrap();
    router.handle(KEY_UP, TAB, COMMAND, false);
    let app = router.handle(KEY_DOWN, 13, COMMAND, false).1.unwrap();
    assert_eq!(app.kind, ActionKind::LaunchApp(2));
    assert_ne!(app.session, switch.session);
    assert_eq!(router.handle(FLAGS_CHANGED, 55, 0, false), (false, None));
    assert_eq!(router.handle(KEY_UP, 13, 0, false), (true, None));
    router.open_search();
    assert_eq!(
        router.handle(KEY_DOWN, 18, COMMAND, false).1.unwrap().kind,
        ActionKind::LaunchApp(0)
    );
}
