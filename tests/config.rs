use winlane::config::{
    Appearance, Config, KEYS, Shortcut, SortOrder, parse_excluded, visible_matches,
};
use winlane::search::WindowInfo;

fn windows() -> Vec<WindowInfo> {
    [
        (1, 10, "Safari", "Zebra", false),
        (2, 20, "Terminal", "Alpha", false),
        (3, 10, "Safari", "Beta", true),
        (4, 30, "Obsidian", "项目笔记", false),
    ]
    .into_iter()
    .map(|(id, pid, app, title, minimized)| WindowInfo {
        id,
        pid,
        app: app.into(),
        title: title.into(),
        minimized,
    })
    .collect()
}

#[test]
fn saved_preferences_round_trip_without_window_data() {
    let config = Config {
        sort: SortOrder::Title,
        appearance: Appearance::Dark,
        excluded_apps: vec!["Safari".into()],
        ..Config::default()
    };
    let json = config.to_json().unwrap();
    assert_eq!(Config::from_json(&json).unwrap(), config);
    assert!(!json.contains("Zebra"));
    assert_eq!(Config::from_json("{}").unwrap(), Config::default());
    assert_eq!(Config::default().shortcut.display(), "⌃I");
    assert!(Config::default().switch_shortcut.is_command_tab());
}

#[test]
fn saved_search_shortcuts_survive_default_changes() {
    for (shortcut, expected) in [
        (
            r#"{"control":true,"option":true,"shift":false,"command":false,"key":"Space"}"#,
            "⌃⌥Space",
        ),
        (
            r#"{"control":false,"option":true,"shift":false,"command":false,"key":"KeyU"}"#,
            "⌥U",
        ),
    ] {
        let json = format!(r#"{{"shortcut":{shortcut}}}"#);
        let config = Config::from_json(&json).unwrap();
        assert_eq!(config.shortcut.display(), expected);
        assert!(config.switch_shortcut.is_command_tab());
        assert_eq!(
            Config::from_json(&config.to_json().unwrap()).unwrap(),
            config
        );
    }
}

#[test]
fn background_opacity_preserves_legacy_defaults_and_validates_percentages() {
    assert_eq!(Config::from_json("{}").unwrap().background_opacity, 100);
    for value in [0, 37, 100] {
        let config = Config {
            background_opacity: value,
            ..Config::default()
        };
        assert_eq!(
            Config::from_json(&config.to_json().unwrap()).unwrap(),
            config
        );
    }
    for value in ["-1", "101", "256", "50.5", "null"] {
        assert!(Config::from_json(&format!(r#"{{"background_opacity":{value}}}"#)).is_err());
    }
    assert!(
        Config {
            background_opacity: 101,
            ..Config::default()
        }
        .to_json()
        .is_err()
    );
}

#[test]
fn legacy_launch_preference_is_ignored_without_losing_other_settings() {
    for show_on_launch in [false, true] {
        let json = format!(
            r#"{{"show_on_launch":{show_on_launch},"sort":"Title","excluded_apps":["Safari"]}}"#
        );
        let config = Config::from_json(&json).unwrap();
        assert_eq!(config.sort, SortOrder::Title);
        assert_eq!(config.excluded_apps, ["Safari"]);
        assert!(!config.to_json().unwrap().contains("show_on_launch"));
    }
}

#[test]
fn invalid_settings_fail_without_silently_using_a_different_shortcut() {
    assert!(Config::from_json(r#"{"shortcut":{"key":"Invalid"}}"#).is_err());
    assert!(Config::from_json("broken").is_err());
    let unsafe_shortcut = Shortcut {
        control: false,
        option: false,
        command: true,
        key: "KeyQ".into(),
        ..Shortcut::default()
    };
    assert!(unsafe_shortcut.hotkey().is_err());
    for key in KEYS {
        assert!(
            Shortcut {
                key: key.to_string(),
                ..Shortcut::default()
            }
            .hotkey()
            .is_ok()
        );
    }
}

#[test]
fn command_tab_is_accepted_and_survives_saved_preferences() {
    let config = Config {
        shortcut: Shortcut {
            control: false,
            option: false,
            shift: false,
            command: true,
            key: "Tab".into(),
        },
        switch_shortcut: Shortcut {
            key: "Tab".into(),
            ..Shortcut::default()
        },
        ..Config::default()
    };
    assert!(config.shortcut.hotkey().is_ok());
    assert_eq!(
        Config::from_json(&config.to_json().unwrap()).unwrap(),
        config
    );
}

#[test]
fn filters_compose_without_changing_the_result_identity() {
    let config = Config {
        include_minimized: false,
        excluded_apps: vec!["terminal".into()],
        ..Config::default()
    };
    let items = windows();
    assert_eq!(
        visible_matches(&items, "", None, &config, None, &[], 0),
        vec![0, 3]
    );
    assert_eq!(
        visible_matches(&items, "", None, &config, Some(10), &[], 0),
        vec![0]
    );
    assert!(visible_matches(&items, "beta", None, &config, None, &[], 0).is_empty());
    assert!(visible_matches(&items, "", None, &config, Some(99), &[], 0).is_empty());
}

#[test]
fn sorting_preserves_search_relevance_and_original_indices() {
    let config = Config {
        sort: SortOrder::Application,
        ..Config::default()
    };
    let items = windows();
    assert_eq!(
        visible_matches(&items, "", None, &config, None, &[], 0),
        vec![3, 2, 0, 1]
    );
    assert_eq!(
        visible_matches(&items, "safari beta", None, &config, None, &[], 0),
        vec![2]
    );
    let config = Config {
        sort: SortOrder::Title,
        ..config
    };
    assert_eq!(
        visible_matches(&items, "", None, &config, None, &[], 0),
        vec![1, 2, 0, 3]
    );
    let config = Config::default();
    assert_eq!(
        visible_matches(&items, "", None, &config, None, &[3, 2], 10),
        vec![2, 1, 0, 3]
    );
}

#[test]
fn exclusions_accept_chinese_punctuation_and_deduplicate_exact_app_names() {
    assert_eq!(
        parse_excluded("Safari，Terminal\nsafari, 备忘录 , ,"),
        vec!["Safari", "Terminal", "备忘录"]
    );
    let config = Config {
        excluded_apps: parse_excluded("Term"),
        ..Config::default()
    };
    assert_eq!(
        visible_matches(&windows(), "Terminal", None, &config, None, &[], 0),
        vec![1]
    );
}

#[test]
fn recent_search_preserves_window_history_instead_of_match_scores() {
    let items: Vec<_> = [
        (1, 20, "Notion", "CKB Dev Log"),
        (2, 10, "Code", "snapshot2.rs — ckb"),
        (3, 10, "Code", "ckb.rs — fiber"),
        (4, 30, "Browser", "Unrelated"),
    ]
    .into_iter()
    .map(|(id, pid, app, title)| WindowInfo {
        id,
        pid,
        app: app.into(),
        title: title.into(),
        minimized: false,
    })
    .collect();
    let config = Config::default();
    for query in ["c", "ck", "ckb", " CKB "] {
        for preferred in [None, Some(1), Some(3)] {
            assert_eq!(
                visible_matches(&items, query, preferred, &config, None, &[2, 1, 3, 4], 10),
                vec![1, 0, 2],
                "typing must filter the recent list without reordering it"
            );
        }
    }
    assert_eq!(
        visible_matches(&items, "code ckb", None, &config, None, &[3, 1, 2], 10),
        vec![2, 1],
        "windows from the same app must have independent recency"
    );
    assert!(visible_matches(&items, "missing", None, &config, None, &[2, 1, 3], 10).is_empty());
    assert!(
        visible_matches(
            &items,
            &"c".repeat(129),
            None,
            &config,
            None,
            &[2, 1, 3],
            10
        )
        .is_empty()
    );
}

#[test]
fn switch_delay_migrates_and_bounds_the_hold_interval() {
    assert_eq!(Config::from_json("{}").unwrap().switch_delay_ms, 100);
    for delay in [0, 100, 350, 1000] {
        let config = Config {
            switch_delay_ms: delay,
            ..Config::default()
        };
        assert_eq!(
            Config::from_json(&config.to_json().unwrap()).unwrap(),
            config
        );
    }
    assert!(
        Config {
            switch_delay_ms: 1001,
            ..Config::default()
        }
        .validate()
        .is_err()
    );
}
