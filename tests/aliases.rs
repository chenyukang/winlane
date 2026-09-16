use std::collections::{BTreeSet, HashMap};
use winlane::aliases::{AliasInput, AliasMatch, Aliases, AppIdentity};
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

fn window(id: u64, pid: i32) -> WindowInfo {
    WindowInfo {
        id,
        pid,
        app: "App".into(),
        title: "Project".into(),
        minimized: false,
    }
}

#[test]
fn switch_prefix_matches_unique_windows_and_requires_disambiguation() {
    let mut aliases = Aliases::from_json(r#"{"zed":"z","zulip":"zu","zoom":"zo"}"#).unwrap();
    let identities = HashMap::from([
        (1, app("zed", "Zed")),
        (2, app("zulip", "Zulip")),
        (3, app("zoom", "Zoom")),
    ]);
    aliases.ensure_windows(&[window(1, 1), window(2, 2), window(3, 3)], &identities);
    assert_eq!(aliases.match_windows("z", &[99, 2]), AliasMatch::Matched(1));
    assert_eq!(aliases.match_windows("z", &[2, 1]), AliasMatch::Matched(1));
    assert_eq!(aliases.match_windows("z", &[3, 2]), AliasMatch::Ambiguous);
    assert_eq!(aliases.match_windows("zu", &[3, 2]), AliasMatch::Matched(1));
    assert_eq!(aliases.match_windows("z", &[99]), AliasMatch::Missing);
    assert_eq!(aliases.match_windows("", &[2]), AliasMatch::Missing);
    assert_eq!(aliases.match_windows(" Z ", &[2]), AliasMatch::Matched(0));
    aliases.ensure_windows(&[window(2, 2), window(4, 2)], &identities);
    assert_ne!(aliases.for_window(2), aliases.for_window(4));
    assert_eq!(aliases.match_windows("z", &[2, 4]), AliasMatch::Ambiguous);
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
        r#"{"apps":{"code":"co"},"windows":{"1":{"app":"code","alias":"co"},"2":{"app":"code","alias":"co"}}}"#,
        r#"{"apps":{"code":"co","chat":"c"},"windows":{"1":{"app":"code","alias":"c"}}}"#,
        r#"{"apps":{"code":"co"},"windows":{"1":{"app":"other","alias":"o"}}}"#,
    ] {
        assert!(Aliases::from_json(json).is_err(), "{json}");
    }
}

#[test]
fn each_window_keeps_its_alias_across_order_title_changes_and_winlane_restart() {
    let identities = HashMap::from([(42, app("code", "Code")), (43, app("chat", "ChatGPT"))]);
    let mut windows = vec![
        window(11, 42),
        window(12, 42),
        window(13, 42),
        window(14, 43),
    ];
    let mut aliases = Aliases::from_json(r#"{"code":"co","chat":"c"}"#).unwrap();
    assert!(aliases.ensure_windows(&windows, &identities));
    assert_eq!(aliases.for_window(11), Some("co"));
    let assigned: BTreeSet<_> = windows
        .iter()
        .map(|window| aliases.for_window(window.id).unwrap())
        .collect();
    assert_eq!(assigned.len(), windows.len());
    let saved = aliases.to_json();
    assert!(!saved.contains("Project"));
    let mut restarted = Aliases::from_json(&saved).unwrap();
    windows.reverse();
    windows[0].title = "Completely different title".into();
    assert!(!restarted.ensure_windows(&windows, &identities));
    assert_eq!(aliases, restarted);
    let order = vec![3, 1, 0, 2];
    for (position, &index) in order.iter().enumerate() {
        let id = windows[index].id;
        let alias = aliases.for_window(id).unwrap();
        assert_eq!(
            aliases.filter_order(alias, &order, &windows),
            Some(vec![index])
        );
        let ordered: Vec<_> = order.iter().map(|&index| windows[index].id).collect();
        assert_eq!(
            aliases.match_windows(alias, &ordered),
            AliasMatch::Matched(position)
        );
    }
}

#[test]
fn alias_search_lists_sibling_windows_after_the_target_and_respects_filters() {
    let identities = HashMap::from([
        (10, app("code", "Code")),
        (11, app("code", "Code")),
        (20, app("other", "Code")),
        (30, app("browser", "Browser")),
    ]);
    let mut windows = vec![
        window(1, 10),
        window(2, 10),
        window(3, 11),
        window(4, 20),
        window(5, 30),
    ];
    windows[1].minimized = true;
    let mut aliases = Aliases::from_json(r#"{"code":"co","other":"x","browser":"b"}"#).unwrap();
    aliases.ensure_windows(&windows, &identities);
    let order = [4, 2, 3, 1, 0];
    assert_eq!(
        aliases.search_order(" Co ", &order, &windows),
        Some(vec![0, 2, 1])
    );
    assert_eq!(aliases.filter_order("co", &order, &windows), Some(vec![0]));
    assert_eq!(
        aliases.match_windows("co", &[5, 3, 4, 2, 1]),
        AliasMatch::Matched(4)
    );
    assert_eq!(aliases.search_order("project", &order, &windows), None);
    assert_eq!(aliases.search_order("co", &[], &windows), Some(vec![]));
    let config = Config {
        include_minimized: false,
        ..Config::default()
    };
    let order = visible_matches(&windows, "", None, &config, None, &[3, 2, 1], 10);
    assert_eq!(
        aliases.search_order("co", &order, &windows),
        Some(vec![0, 2])
    );
    let order = visible_matches(&windows, "", None, &config, Some(11), &[3, 2, 1], 10);
    assert_eq!(aliases.search_order("co", &order, &windows), Some(vec![2]));
    let order = visible_matches(&windows, "", None, &config, Some(20), &[], 10);
    assert_eq!(aliases.search_order("co", &order, &windows), Some(vec![]));
    let config = Config {
        excluded_apps: vec!["App".into()],
        ..config
    };
    let order = visible_matches(&windows, "", None, &config, None, &[], 10);
    assert_eq!(aliases.search_order("co", &order, &windows), Some(vec![]));
    let restored = Aliases::from_json(&aliases.to_json()).unwrap();
    assert_eq!(
        restored.search_order("co", &[4, 2, 3, 1, 0], &windows),
        Some(vec![0, 2, 1])
    );
}

#[test]
fn newly_seen_apps_do_not_steal_window_aliases_and_closed_windows_release_theirs() {
    let mut identities = HashMap::from([(42, app("code", "Code"))]);
    let mut windows = vec![window(11, 42), window(12, 42), window(13, 42)];
    let mut aliases = Aliases::from_json(r#"{"code":"co","chat":"c"}"#).unwrap();
    aliases.ensure_windows(&windows, &identities);
    let extra = aliases.for_window(12).unwrap().to_owned();
    identities.insert(50, app("new-app", &extra.to_uppercase()));
    windows.push(window(50, 50));
    aliases.ensure_windows(&windows, &identities);
    assert_eq!(aliases.for_window(12), Some(extra.as_str()));
    assert_ne!(aliases.for_window(50), Some(extra.as_str()));
    let survivors = [
        (11, aliases.for_window(11).unwrap().to_owned()),
        (13, aliases.for_window(13).unwrap().to_owned()),
    ];
    windows.retain(|window| window.id != 12);
    assert!(aliases.ensure_windows(&windows, &identities));
    assert_eq!(aliases.resolve_window(&extra), None);
    for (id, alias) in survivors {
        assert_eq!(aliases.for_window(id), Some(alias.as_str()));
    }
    windows.push(window(15, 42));
    aliases.ensure_windows(&windows, &identities);
    assert_eq!(aliases.for_window(15), Some(extra.as_str()));
    assert_eq!(aliases, Aliases::from_json(&aliases.to_json()).unwrap());
}

#[test]
fn identical_titles_and_multiple_processes_still_get_distinct_window_aliases() {
    let identities = HashMap::from([(41, app("code", "Code")), (42, app("code", "Code"))]);
    let windows = vec![window(1, 41), window(2, 42)];
    let mut aliases = Aliases::default();
    aliases.ensure_windows(&windows, &identities);
    assert_ne!(aliases.for_window(1), aliases.for_window(2));
    let mut reversed = Aliases::default();
    reversed.ensure_windows(&windows.into_iter().rev().collect::<Vec<_>>(), &identities);
    assert_eq!(aliases, reversed);
}

#[test]
fn alias_search_selects_one_window_and_obeys_filters() {
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
    aliases.ensure_windows(&windows, &identities);
    let config = Config::default();
    let order = visible_matches(&windows, "", None, &config, None, &[2], 0);
    assert_eq!(aliases.filter_order("w", &order, &windows), Some(vec![0]));
    assert_eq!(aliases.filter_order("wa", &order, &windows), Some(vec![2]));
    assert_eq!(aliases.filter_order("project", &order, &windows), None);
    let config = Config {
        include_minimized: false,
        ..config
    };
    let order = visible_matches(&windows, "", None, &config, None, &[], 0);
    assert_eq!(aliases.filter_order("w", &order, &windows), Some(vec![0]));
    let order = visible_matches(&windows, "", None, &config, Some(20), &[], 0);
    assert_eq!(aliases.filter_order("w", &order, &windows), Some(vec![]));
    let config = Config {
        excluded_apps: vec!["微信".into()],
        ..config
    };
    let order = visible_matches(&windows, "", None, &config, None, &[], 0);
    assert_eq!(aliases.filter_order("w", &order, &windows), Some(vec![]));
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
    let identities = HashMap::from([(1, wechat()), (2, app("warp", "Warp"))]);
    aliases.ensure_windows(&[window(1, 1), window(2, 2)], &identities);
    let windows = [99, 2, 1];
    selection.install(windows.len(), Some(0));
    let index = aliases
        .match_windows(input.text(), &windows)
        .position()
        .unwrap();
    selection.select(index);
    assert_eq!(selection.take_commit(), Some(1));
    assert_eq!(selection.take_commit(), None);
    assert_eq!(aliases.match_windows("zz", &windows), AliasMatch::Missing);
    assert_eq!(aliases.match_windows("w", &[2]), AliasMatch::Matched(0));
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
