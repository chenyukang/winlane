use winlane::core::config::{AppShortcut, ApplicationTarget, Config, Shortcut};
use winlane::core::shortcuts::*;
use winlane::features::quicklinks::{self, Quicklink};

fn shortcut(key: &str) -> Shortcut {
    Shortcut {
        command: true,
        control: false,
        option: true,
        shift: false,
        key: key.into(),
    }
}

fn link(id: &str, key: &str) -> Quicklink {
    Quicklink {
        id: id.into(),
        name: id.into(),
        link: "https://example.com/?q={Query}".into(),
        open_with: String::new(),
        shortcut: Some(shortcut(key)),
    }
}

fn config() -> Config {
    Config {
        quicklinks: vec![link("search", "KeyG"), link("docs", "KeyD")],
        ..Config::default()
    }
}

fn router(config: &Config) -> ShortcutRouter {
    ShortcutRouter::new(
        config.shortcut.binding().unwrap(),
        config.switch_shortcut.binding().unwrap(),
    )
    .with_quicklink_shortcuts(config.quicklink_bindings().unwrap())
}

#[test]
fn shortcuts_roundtrip_and_legacy_links_remain_unbound() {
    let old = Config::from_json(
        r#"{"quicklinks":[{"id":"old","name":"Docs","link":"https://example.com"}]}"#,
    )
    .unwrap();
    assert!(old.quicklinks[0].shortcut.is_none());
    assert!(old.quicklink_bindings().unwrap().is_empty());
    assert!(
        serde_json::to_value(&old.quicklinks[0])
            .unwrap()
            .get("shortcut")
            .is_none()
    );
    let config = config();
    assert_eq!(
        Config::from_json(&config.to_json().unwrap()).unwrap(),
        config
    );
    let imported = quicklinks::import_json(&config.quicklinks, r#"[{"name":"search","link":"https://example.com/?q={Query}"},{"name":"New","link":"https://example.test"}]"#).unwrap();
    assert_eq!(imported.skipped, 1);
    assert_eq!(&imported.links[..2], &config.quicklinks);
    assert!(imported.links[2].shortcut.is_none());
}

#[test]
fn quicklinks_conflict_with_all_registered_shortcuts() {
    let mut config = config();
    config.additional_search_shortcuts = vec![shortcut("KeyS")];
    config.app_shortcuts = vec![AppShortcut {
        shortcut: shortcut("KeyA"),
        application: ApplicationTarget {
            bundle_id: "com.example.editor".into(),
            name: "Editor".into(),
            path: "/Applications/Editor.app".into(),
        },
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
        config.quicklinks[1].shortcut.clone().unwrap(),
    ] {
        config.quicklinks[0].shortcut = Some(conflict);
        config
            .to_json()
            .expect_err("conflicting shortcuts must not be saved");
    }
    config.quicklinks[0].shortcut = None;
    config.validate().unwrap();
    for invalid in [
        Shortcut {
            command: false,
            option: false,
            ..shortcut("KeyG")
        },
        shortcut("Invalid"),
    ] {
        config.quicklinks[0].shortcut = Some(invalid);
        assert!(config.validate().is_err());
    }
}

#[test]
fn quicklink_shortcuts_fire_once_and_consume_the_release() {
    let mut router = router(&config());
    let (consumed, action) = router.handle(KEY_DOWN, 5, COMMAND | OPTION, false);
    assert!(consumed);
    assert_eq!(
        action.unwrap().kind,
        ActionKind::OpenQuicklink("search".into())
    );
    assert_eq!(
        router.handle(KEY_DOWN, 5, COMMAND | OPTION, true),
        (true, None)
    );
    assert_eq!(
        router.handle(KEY_DOWN, 5, COMMAND | OPTION, false),
        (true, None)
    );
    assert_eq!(router.handle(FLAGS_CHANGED, 55, 0, false), (false, None));
    assert_eq!(router.handle(KEY_UP, 5, 0, false), (true, None));
    assert_eq!(router.handle(KEY_DOWN, 5, COMMAND, false), (false, None));
    assert_eq!(
        router.handle(KEY_DOWN, 5, COMMAND | OPTION | SHIFT, false),
        (false, None)
    );
    assert_eq!(
        router.handle(KEY_DOWN, 5, COMMAND | OPTION, true),
        (true, None)
    );
}

#[test]
fn quicklink_shortcuts_leave_switching_and_can_resume_argument_search() {
    let mut router = router(&config());
    let switch = router.handle(KEY_DOWN, TAB, COMMAND, false).1.unwrap();
    router.handle(KEY_UP, TAB, COMMAND, false);
    let link = router
        .handle(KEY_DOWN, 5, COMMAND | OPTION, false)
        .1
        .unwrap();
    assert_ne!(link.session, switch.session);
    assert_eq!(router.handle(FLAGS_CHANGED, 55, 0, false), (false, None));
    router.resume_search(link.session);
    router.finish(switch.session);
    let cancel = router.handle(KEY_DOWN, 34, CONTROL, false).1.unwrap();
    assert_eq!(cancel.kind, ActionKind::Cancel);
    assert_eq!(cancel.session, link.session);
}

#[test]
fn bindings_keep_link_identity_when_reordered_removed_or_disabled() {
    let mut config = config();
    config.quicklinks.reverse();
    assert_eq!(
        router(&config)
            .handle(KEY_DOWN, 5, COMMAND | OPTION, false)
            .1
            .unwrap()
            .kind,
        ActionKind::OpenQuicklink("search".into())
    );
    config.quicklinks[1].shortcut = None;
    assert_eq!(
        router(&config).handle(KEY_DOWN, 5, COMMAND | OPTION, false),
        (false, None)
    );
    config.quicklinks.remove(0);
    assert!(config.quicklink_bindings().unwrap().is_empty());
    assert_eq!(
        router(&config).handle(KEY_DOWN, 2, COMMAND | OPTION, false),
        (false, None)
    );
}
