use winlane::core::config::{
    Appearance, Config, DisplayDensity, KEYS, PanelDisplayTarget, Shortcut, SortOrder,
    parse_excluded, visible_matches,
};
use winlane::core::search::WindowInfo;

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
        display_density: DisplayDensity::Compact,
        panel_display_target: PanelDisplayTarget::ActiveDisplay,
        excluded_apps: vec!["Safari".into()],
        ..Config::default()
    };
    let json = config.to_json().unwrap();
    assert_eq!(Config::from_json(&json).unwrap(), config);
    assert!(!json.contains("Zebra"));
    assert_eq!(Config::from_json("{}").unwrap(), Config::default());
    assert_eq!(
        Config::from_json("{}").unwrap().panel_display_target,
        PanelDisplayTarget::AllDisplays
    );
    assert_eq!(
        Config::from_json("{}").unwrap().display_density,
        DisplayDensity::Normal
    );
    assert_eq!(Config::default().shortcut.display(), "⌃I");
    assert!(Config::default().switch_shortcut.is_command_tab());
}

#[test]
fn logging_defaults_and_legacy_debug_migrate_without_overriding_new_settings() {
    use winlane::core::logging::{DEFAULT_FILE_PATH, Level};
    let config = Config::from_json("{}").unwrap();
    assert_eq!(config.logging.level, Level::Info);
    assert_eq!(config.logging.file_path, DEFAULT_FILE_PATH);
    assert_eq!(
        Config::from_json(r#"{"debug_logging":true}"#)
            .unwrap()
            .logging
            .level,
        Level::Debug
    );
    assert_eq!(
        Config::from_json(r#"{"debug_logging":false}"#)
            .unwrap()
            .logging
            .level,
        Level::Info
    );
    let config = Config::from_json(
        r#"{"debug_logging":true,"logging":{"level":"off","file_path":"/tmp/example.log"}}"#,
    )
    .unwrap();
    assert_eq!(config.logging.level, Level::Off);
    let json = config.to_json().unwrap();
    assert!(!json.contains("debug_logging"));
    assert_eq!(Config::from_json(&json).unwrap(), config);
}

#[test]
fn logging_levels_and_paths_validate_and_round_trip() {
    use winlane::core::logging::Level;
    for level in [
        Level::Off,
        Level::Error,
        Level::Warn,
        Level::Info,
        Level::Debug,
    ] {
        let mut config = Config::default();
        config.logging.level = level;
        config.logging.file_path = "~/Logs/example.log".into();
        assert_eq!(
            Config::from_json(&config.to_json().unwrap()).unwrap(),
            config
        );
        assert_eq!(
            config
                .logging
                .resolve_path(Some(std::path::Path::new("/Users/example")))
                .unwrap(),
            std::path::Path::new("/Users/example/Logs/example.log")
        );
        assert!(config.logging.resolve_path(None).is_err());
        config.logging.file_path = "/tmp/example.log".into();
        assert_eq!(
            config.logging.resolve_path(None).unwrap(),
            std::path::Path::new("/tmp/example.log")
        );
    }
    for path in [
        "",
        "relative.log",
        "~other/file.log",
        "~/",
        "/",
        "/tmp/",
        "/tmp/..",
        "/tmp/.",
        "/tmp/bad\nfile",
        "/tmp/bad\0file",
    ] {
        let mut config = Config::default();
        config.logging.file_path = path.into();
        assert!(config.to_json().is_err(), "accepted {path:?}");
    }
    assert!(Config::from_json(r#"{"logging":{"level":"unknown"}}"#).is_err());
}

#[test]
fn usage_hints_preserve_legacy_defaults_and_saved_choice() {
    assert!(Config::from_json("{}").unwrap().show_usage_hints);
    for show_usage_hints in [false, true] {
        let config = Config {
            show_usage_hints,
            ..Config::default()
        };
        assert_eq!(
            Config::from_json(&config.to_json().unwrap()).unwrap(),
            config
        );
    }
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
    for query in ["ck", "ckb", " CKB "] {
        for preferred in [None, Some(1), Some(3)] {
            assert_eq!(
                visible_matches(&items, query, preferred, &config, None, &[2, 1, 3, 4], 10),
                vec![1, 0, 2],
                "typing must filter the recent list without reordering it"
            );
        }
    }
    assert_eq!(
        visible_matches(&items, "c", None, &config, None, &[2, 1, 3, 4], 10),
        vec![1, 2, 0],
        "app-name prefixes precede title matches while preserving per-window recency"
    );
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
fn recent_search_prioritizes_direct_title_matches_over_scattered_characters() {
    let items: Vec<_> = [
        (1, "Google Chrome", "[WIP]: LSP support · Pull Request #42"),
        (2, "Code", "unused.rs (Working Tree) — rust"),
        (3, "Zulip", "Zulip - rust-lang"),
        (4, "Editor", "Reusing tests"),
    ]
    .into_iter()
    .map(|(id, app, title)| WindowInfo {
        id,
        pid: id as i32,
        app: app.into(),
        title: title.into(),
        minimized: false,
    })
    .collect();
    for query in ["rust", " RUST "] {
        for preferred in [None, Some(1)] {
            assert_eq!(
                visible_matches(
                    &items,
                    query,
                    preferred,
                    &Config::default(),
                    None,
                    &[1, 2, 3, 4],
                    1,
                ),
                vec![1, 2, 3, 0],
                "recent fuzzy matches must follow titles containing the search text"
            );
            assert_eq!(
                visible_matches(
                    &items,
                    query,
                    preferred,
                    &Config::default(),
                    None,
                    &[4, 3, 1, 2],
                    4,
                ),
                vec![2, 1, 3, 0],
                "direct matches retain recency; tighter fuzzy matches precede scattered ones"
            );
        }
    }
    assert_eq!(
        visible_matches(&items, "", None, &Config::default(), None, &[1, 2, 3, 4], 1,),
        vec![0, 1, 2, 3],
    );
}

#[test]
fn fuzzy_quality_precedes_recency_but_not_direct_matches() {
    let items: Vec<_> = [
        (1, "Google Chrome", "First window"),
        (2, "Google Chrome", "Second window"),
        (3, "Editor", "cxhxorxmxe"),
        (4, "Editor", "Chrome notes"),
        (5, "Chorme", "Exact app name"),
        (6, "Editor", "Chorme notes"),
    ]
    .into_iter()
    .map(|(id, app, title)| WindowInfo {
        id,
        pid: id as i32,
        app: app.into(),
        title: title.into(),
        minimized: false,
    })
    .collect();
    for preferred in [None, Some(3)] {
        assert_eq!(
            visible_matches(
                &items,
                "chorme",
                preferred,
                &Config::default(),
                None,
                &[3, 4, 2, 1, 6, 5],
                3,
            ),
            [4, 5, 1, 0, 3, 2],
            "direct hits lead; app typos precede title typos and scattered letters; equal-quality windows retain recency"
        );
    }
    assert!(
        visible_matches(
            &items,
            "chorme missing",
            None,
            &Config::default(),
            None,
            &[],
            0,
        )
        .is_empty()
    );
}

#[test]
fn app_name_matches_precede_recent_title_matches_without_reordering_each_group() {
    let items: Vec<_> = [
        (
            1,
            10,
            "Code",
            "localization.rs (Working Tree) (localization.rs) — windowlane",
        ),
        (2, 20, "Notion", "CKB Dev Log"),
        (3, 30, "Browser", "notion"),
        (4, 20, "Notion", "Notes"),
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
    let recent = [3, 1, 4, 2];
    for query in ["notion", "NOTION", "  Notion\t", "noti", "NOT"] {
        for preferred in [None, Some(1), Some(3)] {
            assert_eq!(
                visible_matches(&items, query, preferred, &config, None, &recent, 10),
                vec![3, 1, 2, 0],
                "app names must lead; each group's windows retain their own recency"
            );
        }
    }
    assert_eq!(
        visible_matches(&items, "", None, &config, None, &recent, 10),
        vec![2, 0, 3, 1]
    );
    assert_eq!(
        visible_matches(&items, "notion", None, &config, Some(10), &recent, 10),
        vec![0],
        "app matching must not bypass current-app scope"
    );
    let config = Config {
        excluded_apps: vec!["Notion".into()],
        ..config
    };
    assert_eq!(
        visible_matches(&items, "notion", None, &config, None, &recent, 10),
        vec![2, 0],
        "excluded apps must stay excluded"
    );
    let items = [
        WindowInfo {
            id: 1,
            pid: 10,
            app: "Browser".into(),
            title: "System Settings".into(),
            minimized: false,
        },
        WindowInfo {
            id: 2,
            pid: 20,
            app: "System Settings".into(),
            title: "Privacy".into(),
            minimized: false,
        },
    ];
    assert_eq!(
        visible_matches(
            &items,
            " system\tSETTINGS  ",
            None,
            &Config::default(),
            None,
            &[1, 2],
            10
        ),
        vec![1, 0],
        "multiword names normalize case and whitespace like other searches"
    );
    let items: Vec<_> = [
        ("Editor", "Chrome"),
        ("Google Chrome", "Documentation"),
        ("Chrome", "About"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (app, title))| WindowInfo {
        id: index as u64,
        pid: index as i32,
        app: app.into(),
        title: title.into(),
        minimized: false,
    })
    .collect();
    assert_eq!(
        visible_matches(
            &items,
            "chrome",
            None,
            &Config::default(),
            None,
            &[0, 1, 2],
            0
        ),
        vec![2, 1, 0],
        "complete names outrank partial app names, which outrank title matches"
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

#[test]
fn additional_search_shortcuts_migrate_persist_and_validate_every_conflict() {
    use winlane::core::config::{AppShortcut, ApplicationTarget, MAX_SEARCH_SHORTCUTS};
    let mut config =
        Config::from_json(r#"{"shortcut":{"control":false,"command":true,"key":"Space"}}"#)
            .unwrap();
    assert!(config.additional_search_shortcuts.is_empty());
    config.additional_search_shortcuts.push(Shortcut::default());
    assert_eq!(
        Config::from_json(&config.to_json().unwrap()).unwrap(),
        config
    );
    assert_eq!(config.search_shortcuts_display(), "⌘Space / ⌃I");
    assert_eq!(config.search_bindings().unwrap().len(), 2);
    let mut bad = config.clone();
    bad.additional_search_shortcuts
        .push(config.shortcut.clone());
    assert!(bad.validate().is_err());
    bad = config.clone();
    bad.additional_search_shortcuts.push(Shortcut::default());
    assert!(bad.validate().is_err());
    for shift in [false, true] {
        bad = config.clone();
        bad.switch_shortcut = Shortcut {
            key: "Tab".into(),
            ..Shortcut::default()
        };
        bad.additional_search_shortcuts.push(Shortcut {
            key: "Tab".into(),
            shift,
            ..Shortcut::default()
        });
        assert!(bad.validate().is_err());
    }
    bad = config.clone();
    bad.app_shortcuts.push(AppShortcut {
        application: ApplicationTarget {
            name: "Example".into(),
            path: "/Applications/Example.app".into(),
            bundle_id: "com.example.app".into(),
        },
        shortcut: Shortcut::default(),
    });
    assert!(bad.validate().is_err());
    bad = config.clone();
    bad.additional_search_shortcuts[0].key = "Invalid".into();
    assert!(bad.validate().is_err());
    config.additional_search_shortcuts = (0..MAX_SEARCH_SHORTCUTS - 1)
        .map(|n| Shortcut {
            key: format!("F{}", n + 1),
            ..Shortcut::default()
        })
        .collect();
    assert!(config.validate().is_ok());
    config.additional_search_shortcuts.push(Shortcut {
        key: "F12".into(),
        ..Shortcut::default()
    });
    assert!(config.validate().is_err());
    assert!(Config::default().additional_search_shortcuts.is_empty());
    assert!(
        !Config::default()
            .to_json()
            .unwrap()
            .contains("additional_search_shortcuts")
    );
}
