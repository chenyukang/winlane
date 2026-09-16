use std::collections::{HashMap, HashSet};
use winlane::aliases::{AliasMatch, Aliases, AppIdentity};
use winlane::config::{AliasRule, ApplicationTarget, Config};
use winlane::search::WindowInfo;

fn rule(alias: &str, app: &str, title: &str) -> AliasRule {
    AliasRule {
        alias: alias.into(),
        application: ApplicationTarget {
            bundle_id: app.into(),
            name: app.into(),
            path: format!("/Applications/{app}.app"),
        },
        title_contains: title.into(),
    }
}
fn fixture() -> (Vec<WindowInfo>, HashMap<i32, AppIdentity>, Aliases) {
    let windows: Vec<_> = [
        (1, 10, "main.rs — ckb"),
        (2, 10, "lib.rs — rust"),
        (3, 10, "notes"),
        (4, 20, "docs"),
        (5, 30, "chat"),
    ]
    .into_iter()
    .map(|(id, pid, title)| WindowInfo {
        id,
        pid,
        title: title.into(),
        app: format!("App {pid}"),
        minimized: false,
    })
    .collect();
    let identities = [
        (10, "code", "Code"),
        (20, "browser", "Chrome"),
        (30, "chat", "Chat"),
    ]
    .into_iter()
    .map(|(pid, id, name)| {
        (
            pid,
            AppIdentity {
                id: id.into(),
                english_name: name.into(),
            },
        )
    })
    .collect();
    let mut aliases = Aliases::default();
    aliases.ensure_windows(&windows, &identities);
    (windows, identities, aliases)
}

#[test]
fn custom_rules_override_collisions_without_changing_automatic_storage() {
    let (windows, ids, auto) = fixture();
    let rules = vec![
        rule("ck", "code", "CKB"),
        rule("ru", "code", "rust"),
        rule("c", "chat", ""),
        rule("v", "code", ""),
    ];
    let saved = auto.to_json();
    let effective = auto.with_rules(&windows, &ids, &rules);
    assert_eq!(effective.for_window(1), Some("ck"));
    assert_eq!(effective.for_window(2), Some("ru"));
    assert_eq!(effective.for_window(3), Some("v"));
    assert_eq!(effective.for_window(5), Some("c"));
    assert_eq!(
        effective.match_windows("ck", &[5, 2, 1, 3, 4]),
        AliasMatch::Matched(2)
    );
    assert_eq!(
        effective.filter_order("ru", &[4, 3, 2, 1, 0], &windows),
        Some(vec![1])
    );
    assert_eq!(
        windows
            .iter()
            .filter_map(|w| effective.for_window(w.id))
            .collect::<HashSet<_>>()
            .len(),
        windows.len()
    );
    assert_eq!(auto.to_json(), saved);
    assert_eq!(auto.with_rules(&windows, &ids, &[]), auto);
}

#[test]
fn rules_survive_new_window_ids_titles_and_configuration_round_trip() {
    let (mut windows, ids, mut auto) = fixture();
    let config = Config {
        alias_rules: vec![rule("ck", "code", "ckb"), rule("ru", "code", "rust")],
        ..Config::default()
    };
    let restored = Config::from_json(&config.to_json().unwrap()).unwrap();
    windows[0].id = 100;
    windows[0].title = "README.md — ckb".into();
    auto = Aliases::from_json(&auto.to_json()).unwrap();
    auto.ensure_windows(&windows, &ids);
    let effective = auto.with_rules(&windows, &ids, &restored.alias_rules);
    assert_eq!(effective.resolve_window("ck"), Some(100));
    windows.reverse();
    assert_eq!(
        auto.with_rules(&windows, &ids, &restored.alias_rules),
        effective
    );
    windows.retain(|w| w.id != 100);
    auto.ensure_windows(&windows, &ids);
    let closed = auto.with_rules(&windows, &ids, &restored.alias_rules);
    assert!(closed.is_alias("ck"));
    assert_eq!(
        closed.match_windows("ck", &[2, 3, 4, 5]),
        AliasMatch::Missing
    );
    assert!(
        windows
            .iter()
            .all(|w| closed.for_window(w.id) != Some("ck"))
    );
    assert_eq!(
        closed.filter_order("ck", &[0, 1, 2, 3], &windows),
        Some(vec![])
    );
}

#[test]
fn more_specific_rules_win_and_multiple_matches_remain_unique() {
    let (mut windows, ids, auto) = fixture();
    windows[2].title = "test.rs — ckb".into();
    let rules = [
        rule("p", "code", "ckb"),
        rule("m", "code", "main.rs — ckb"),
        rule("v", "code", ""),
    ];
    let effective = auto.with_rules(&windows, &ids, &rules);
    assert_eq!(effective.for_window(1), Some("m"));
    assert_eq!(effective.for_window(3), Some("p"));
    assert_eq!(effective.for_window(2), Some("v"));
    let all_project = [
        rule("a", "code", "main.rs"),
        rule("b", "code", "lib.rs"),
        rule("d", "code", "test.rs"),
        rule("v", "code", ""),
    ];
    let effective = auto.with_rules(&windows, &ids, &all_project);
    assert_eq!(
        effective.match_windows("v", &[4, 2, 1, 3]),
        AliasMatch::Matched(1)
    );
}

#[test]
fn rules_validate_aliases_targets_duplicates_and_legacy_settings() {
    assert!(Config::from_json("{}").unwrap().alias_rules.is_empty());
    for alias in ["", "abc", "A", "1", "中"] {
        assert!(
            Config {
                alias_rules: vec![rule(alias, "code", "")],
                ..Config::default()
            }
            .validate()
            .is_err()
        );
    }
    for rules in [
        vec![rule("a", "code", "ckb"), rule("a", "chat", "")],
        vec![rule("a", "code", "ckb"), rule("b", "code", "CKB")],
        vec![rule("a", "code", " x ")],
    ] {
        assert!(
            Config {
                alias_rules: rules,
                ..Config::default()
            }
            .validate()
            .is_err()
        );
    }
    let (_, ids, auto) = fixture();
    let effective = auto.with_rules(&[], &ids, &[rule("w", "chat", "")]);
    assert_eq!(
        effective.resolve("w"),
        Some("chat"),
        "app aliases can resolve before launch"
    );
}

#[test]
fn unavailable_custom_aliases_do_not_fall_through_to_automatic_prefixes() {
    let (windows, ids, auto) = fixture();
    let effective = auto.with_rules(&windows, &ids, &[rule("c", "code", "closed project")]);
    assert!(windows.iter().any(|window| {
        effective
            .for_window(window.id)
            .is_some_and(|alias| alias.starts_with('c'))
    }));
    assert_eq!(
        effective.match_windows("c", &[1, 2, 3, 4, 5]),
        AliasMatch::Missing
    );
    assert_eq!(
        effective.filter_order("c", &[0, 1, 2, 3, 4], &windows),
        Some(vec![])
    );
}
