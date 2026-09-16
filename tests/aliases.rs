use std::collections::{BTreeSet, HashMap};
use winlane::aliases::{AliasInput, Aliases, AppIdentity, matching_alias_position};
use winlane::config::{Config, Shortcut, visible_matches};
use winlane::search::WindowInfo;
use winlane::shortcuts::*;

fn app(id: &str, name: &str) -> AppIdentity {
    AppIdentity {
        id: id.into(),
        english_name: name.into(),
    }
}
fn wechat() -> AppIdentity {
    app("com.tencent.xinWeChat", "WeChat")
}
fn configured_router() -> ShortcutRouter {
    ShortcutRouter::new(
        Binding {
            key: 32,
            modifiers: CONTROL,
        },
        Binding {
            key: TAB,
            modifiers: COMMAND,
        },
    )
}

#[test]
fn automatic_aliases_are_lowercase_unique_and_deterministic() {
    let apps = vec![
        wechat(),
        app("warp", "Warp"),
        app("code", "Code"),
        app("chat", "ChatGPT"),
        app("chrome", "Google Chrome"),
    ];
    let mut aliases = Aliases::default();
    assert!(aliases.ensure(&apps));
    assert_eq!(aliases.get(&wechat().id), Some("w"));
    assert_eq!(aliases.get("warp"), Some("wa"));
    assert_eq!(aliases.get("code"), Some("co"));
    assert_eq!(aliases.get("chrome"), Some("g"));
    let mut reversed = Aliases::default();
    reversed.ensure(&apps.into_iter().rev().collect::<Vec<_>>());
    assert_eq!(aliases, reversed);
    assert_eq!(aliases.resolve(" W "), Some(wechat().id.as_str()));
}

#[test]
fn restart_localization_process_changes_and_new_apps_preserve_assignments() {
    let mut aliases = Aliases::default();
    aliases.ensure(&[app("code", "Code"), app("warp", "Warp")]);
    let old_code = aliases.get("code").unwrap().to_owned();
    let stored = aliases.to_json();
    let mut restarted = Aliases::from_json(&stored).unwrap();
    assert!(!restarted.ensure(&[app("code", "代码"), app("warp", "Warp")]));
    assert!(restarted.ensure(&[app("code", "New Code Name"), app("other", "Code")]));
    assert_eq!(restarted.get("code"), Some(old_code.as_str()));
    assert_ne!(restarted.get("code"), restarted.get("other"));
    assert_eq!(restarted.get("warp"), aliases.get("warp"));
    assert!(!stored.contains("窗口标题"));
}

#[test]
fn duplicate_app_names_and_many_collisions_never_reassign_existing_aliases() {
    let mut aliases = Aliases::default();
    let apps: Vec<_> = (0..60)
        .map(|i| app(&format!("app{i:03}"), "Example"))
        .collect();
    aliases.ensure(&apps);
    let assigned: BTreeSet<_> = apps
        .iter()
        .map(|app| aliases.get(&app.id).unwrap())
        .collect();
    assert_eq!(assigned.len(), apps.len());
    assert!(
        assigned.iter().all(|alias| (1..=2).contains(&alias.len())
            && alias.bytes().all(|ch| ch.is_ascii_lowercase()))
    );
    assert!(!aliases.ensure(&apps));
}

#[test]
fn malformed_duplicate_and_overlong_saved_aliases_are_rejected() {
    for json in [
        "broken",
        r#"{"app":"abc"}"#,
        r#"{"app":"A"}"#,
        r#"{"app":""}"#,
        r#"{"":"a"}"#,
        r#"{"app":"a","other":"a"}"#,
    ] {
        assert!(Aliases::from_json(json).is_err(), "{json}");
    }
}

#[test]
fn alias_search_uses_identity_preserves_multiple_windows_and_obeys_filters() {
    let windows = vec![
        WindowInfo {
            id: 1,
            pid: 10,
            app: "微信".into(),
            title: "聊天".into(),
            minimized: false,
        },
        WindowInfo {
            id: 2,
            pid: 10,
            app: "微信".into(),
            title: "通讯录".into(),
            minimized: true,
        },
        WindowInfo {
            id: 3,
            pid: 20,
            app: "Warp".into(),
            title: "WeChat project".into(),
            minimized: false,
        },
    ];
    let identities = HashMap::from([(10, wechat()), (20, app("warp", "Warp"))]);
    let mut aliases = Aliases::default();
    aliases.ensure(&identities.values().cloned().collect::<Vec<_>>());
    let config = Config::default();
    let order = visible_matches(&windows, "", None, &config, None, &[2], 0);
    assert_eq!(
        aliases.filter_order("w", &order, &windows, &identities),
        Some(vec![1, 0])
    );
    assert_eq!(
        aliases.filter_order("wa", &order, &windows, &identities),
        Some(vec![2])
    );
    assert_eq!(
        aliases.filter_order("project", &order, &windows, &identities),
        None
    );
    let config = Config {
        include_minimized: false,
        ..config
    };
    let order = visible_matches(&windows, "", None, &config, None, &[], 0);
    assert_eq!(
        aliases.filter_order("w", &order, &windows, &identities),
        Some(vec![0])
    );
    let order = visible_matches(&windows, "", None, &config, Some(20), &[], 0);
    assert_eq!(
        aliases.filter_order("w", &order, &windows, &identities),
        Some(vec![])
    );
    let config = Config {
        excluded_apps: vec!["微信".into()],
        ..config
    };
    let order = visible_matches(&windows, "", None, &config, None, &[], 0);
    assert_eq!(
        aliases.filter_order("w", &order, &windows, &identities),
        Some(vec![])
    );
}

#[test]
fn switch_aliases_select_on_release_without_leaking_command_w_to_apps() {
    let mut router = configured_router();
    let first = router.handle(KEY_DOWN, TAB, COMMAND, false).1.unwrap();
    router.handle(KEY_UP, TAB, COMMAND, false);
    assert_eq!(
        router.handle(KEY_DOWN, 13, COMMAND, false),
        (
            true,
            Some(Action {
                session: first.session,
                kind: ActionKind::Alias('w')
            })
        )
    );
    assert_eq!(router.handle(KEY_DOWN, 13, COMMAND, true), (true, None));
    assert_eq!(
        router.handle(FLAGS_CHANGED, 55, 0, false).1.unwrap().kind,
        ActionKind::Accept
    );
    assert_eq!(router.handle(KEY_UP, 13, 0, false), (true, None));
    assert_eq!(router.handle(KEY_DOWN, 13, COMMAND, false), (false, None));
    router.open_search();
    assert_eq!(router.handle(KEY_DOWN, 13, 0, false), (false, None));
    assert_eq!(router.handle(KEY_DOWN, 13, COMMAND, false), (false, None));
}

#[test]
fn two_letters_backspace_and_navigation_reset_are_predictable() {
    let mut input = AliasInput::default();
    input.push('w');
    assert_eq!(input.text(), "w");
    input.push('a');
    assert_eq!(input.text(), "wa");
    input.pop();
    assert_eq!(input.text(), "w");
    input.push('a');
    input.push('c');
    assert_eq!(input.text(), "c");
    input.clear();
    assert_eq!(input.text(), "");
    input.pop();
    input.push('中');
    assert_eq!(input.text(), "");
    let mut router = configured_router();
    router.enter_switch(COMMAND);
    assert_eq!(
        router.handle(KEY_DOWN, 51, COMMAND, false).1.unwrap().kind,
        ActionKind::AliasBackspace
    );
    router.handle(KEY_UP, 51, COMMAND, false);
    assert_eq!(
        router.handle(KEY_DOWN, 12, COMMAND, false).1.unwrap().kind,
        ActionKind::Alias('q')
    );
    assert_eq!(
        router.handle(KEY_DOWN, 3, COMMAND | CONTROL, false),
        (false, None)
    );
}

#[test]
fn pending_alias_overrides_initial_cycle_after_discovery_and_only_commits_once() {
    let mut aliases = Aliases::default();
    let mut selection = SwitchSelection::new(1);
    let mut input = AliasInput::default();
    input.push('w');
    input.push('a');
    selection.release();
    assert_eq!(selection.take_commit(), None);
    aliases.ensure(&[wechat(), app("warp", "Warp")]);
    let apps = ["code", "warp", "com.tencent.xinWeChat"];
    selection.install(apps.len(), Some(0));
    let index = matching_alias_position(&aliases, input.text(), &apps).unwrap();
    selection.select(index);
    assert_eq!(selection.take_commit(), Some(1));
    assert_eq!(selection.take_commit(), None);
    assert_eq!(matching_alias_position(&aliases, "zz", &apps), None);
    assert_eq!(matching_alias_position(&aliases, "w", &["warp"]), None);
}

#[test]
fn command_backquote_is_configurable_and_keeps_alias_keys_available() {
    let config = Config {
        switch_shortcut: Shortcut {
            key: "Backquote".into(),
            ..Shortcut::switch_default()
        },
        ..Config::default()
    };
    assert!(config.validate().is_ok());
    assert_eq!(
        Config::from_json(&config.to_json().unwrap()).unwrap(),
        config
    );
    let mut router = ShortcutRouter::new(
        config.shortcut.binding().unwrap(),
        config.switch_shortcut.binding().unwrap(),
    );
    assert!(matches!(
        router.handle(KEY_DOWN, 50, COMMAND, false).1.unwrap().kind,
        ActionKind::Switch { fresh: true, .. }
    ));
    assert_eq!(
        router.handle(KEY_DOWN, 13, COMMAND, false).1.unwrap().kind,
        ActionKind::Alias('w')
    );
}
