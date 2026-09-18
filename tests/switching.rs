use winlane::config::{Config, KEYS, Shortcut};
use winlane::shortcuts::*;

fn router() -> ShortcutRouter {
    ShortcutRouter::new(
        Binding {
            key: TAB,
            modifiers: CONTROL,
        },
        Binding {
            key: TAB,
            modifiers: COMMAND,
        },
    )
}

fn action(router: &mut ShortcutRouter, event: u32, key: i64, flags: u64) -> Action {
    router.handle(event, key, flags, false).1.unwrap()
}

#[test]
fn switch_cycles_and_commits_once_on_primary_modifier_release() {
    let mut router = router();
    let first = action(&mut router, KEY_DOWN, TAB, COMMAND);
    assert_eq!(
        first.kind,
        ActionKind::Switch {
            direction: 1,
            fresh: true
        }
    );
    assert_eq!(
        router.handle(KEY_DOWN, TAB, COMMAND, true),
        (
            true,
            Some(Action {
                session: first.session,
                kind: ActionKind::Switch {
                    direction: 1,
                    fresh: false
                },
            })
        )
    );
    assert_eq!(router.handle(KEY_UP, TAB, COMMAND, false), (true, None));
    let next = action(&mut router, KEY_DOWN, TAB, COMMAND | SHIFT);
    assert_eq!(next.session, first.session);
    assert_eq!(
        next.kind,
        ActionKind::Switch {
            direction: -1,
            fresh: false
        }
    );
    assert_eq!(
        router.handle(FLAGS_CHANGED, 56, COMMAND, false),
        (false, None)
    );
    assert_eq!(
        action(&mut router, FLAGS_CHANGED, 55, 0).kind,
        ActionKind::Accept
    );
    assert_eq!(router.handle(FLAGS_CHANGED, 55, 0, false), (false, None));
    assert_eq!(router.handle(KEY_UP, TAB, 0, false), (true, None));
}

#[test]
fn space_enters_search_and_releasing_command_does_not_accept() {
    let mut router = router();
    action(&mut router, KEY_DOWN, TAB, COMMAND);
    router.handle(KEY_UP, TAB, COMMAND, false);
    assert_eq!(
        action(&mut router, KEY_DOWN, SPACE, COMMAND).kind,
        ActionKind::Search
    );
    assert_eq!(router.handle(FLAGS_CHANGED, 55, 0, false), (false, None));
    assert_eq!(router.handle(KEY_UP, SPACE, 0, false), (true, None));
    assert_eq!(router.handle(KEY_DOWN, SPACE, 0, false), (false, None));
}

#[test]
fn held_navigation_matches_repeated_presses_including_reverse_and_wraparound() {
    for (switch_key, modifiers, navigation_key) in [
        (TAB, COMMAND, TAB),
        (50, COMMAND, 50),
        (50, COMMAND, TAB),
        (32, CONTROL, 32),
        (50, COMMAND, 125),
        (50, COMMAND, 126),
    ] {
        let mut held = ShortcutRouter::new(
            Binding {
                key: 34,
                modifiers: CONTROL,
            },
            Binding {
                key: switch_key,
                modifiers,
            },
        );
        let first = action(&mut held, KEY_DOWN, switch_key, modifiers);
        held.handle(KEY_UP, switch_key, modifiers, false);
        let mut tapped = ShortcutRouter::new(
            Binding {
                key: 34,
                modifiers: CONTROL,
            },
            Binding {
                key: switch_key,
                modifiers,
            },
        );
        action(&mut tapped, KEY_DOWN, switch_key, modifiers);
        tapped.handle(KEY_UP, switch_key, modifiers, false);
        let mut selection = SwitchSelection::new(1);
        selection.install(4, Some(0));
        let mut expected = 1_isize;
        for (index, flags) in [modifiers; 5]
            .into_iter()
            .chain([modifiers | SHIFT; 5])
            .enumerate()
        {
            let actual = held.handle(KEY_DOWN, navigation_key, flags, index != 0);
            let expected_action = tapped.handle(KEY_DOWN, navigation_key, flags, false);
            assert_eq!(actual, expected_action);
            let Some(Action {
                session,
                kind: ActionKind::Switch { direction, fresh },
            }) = actual.1
            else {
                panic!("holding navigation must keep moving selection");
            };
            assert_eq!(session, first.session);
            assert!(!fresh);
            selection.step(direction);
            expected = (expected + isize::from(direction)).rem_euclid(4);
            assert_eq!(selection.selected(), Some(expected as usize));
            tapped.handle(KEY_UP, navigation_key, flags, false);
        }
        assert_eq!(
            action(&mut held, FLAGS_CHANGED, 0, 0).kind,
            ActionKind::Accept
        );
        selection.release();
        assert_eq!(selection.take_commit(), Some(expected as usize));
        assert_eq!(selection.take_commit(), None);
        assert_eq!(
            held.handle(KEY_DOWN, navigation_key, modifiers, true),
            (true, None)
        );
        assert_eq!(held.handle(KEY_UP, navigation_key, 0, false), (true, None));
    }
}

#[test]
fn held_switch_key_stops_navigating_after_search_or_cancel() {
    for key in [SPACE, ESCAPE, 36] {
        let mut router = router();
        action(&mut router, KEY_DOWN, TAB, COMMAND);
        action(&mut router, KEY_DOWN, key, COMMAND);
        for _ in 0..3 {
            assert_eq!(router.handle(KEY_DOWN, TAB, COMMAND, true), (true, None));
            assert_eq!(router.handle(KEY_DOWN, key, COMMAND, true), (true, None));
        }
        assert_eq!(router.handle(FLAGS_CHANGED, 55, 0, false), (false, None));
        assert_eq!(router.handle(KEY_UP, TAB, 0, false), (true, None));
    }
}

#[test]
fn space_from_search_without_held_modifier_waits_for_a_new_hold() {
    let mut router = router();
    router.open_search();
    let switched = router.enter_switch(0);
    assert_eq!(
        switched.kind,
        ActionKind::Switch {
            direction: 0,
            fresh: true
        }
    );
    assert_eq!(router.handle(FLAGS_CHANGED, 56, 0, false), (false, None));
    assert_eq!(
        router.handle(FLAGS_CHANGED, 55, COMMAND, false),
        (false, None)
    );
    assert_eq!(
        action(&mut router, FLAGS_CHANGED, 55, 0).kind,
        ActionKind::Accept
    );
}

#[test]
fn custom_switch_shortcut_tracks_control_and_keeps_search_independent() {
    let mut router = ShortcutRouter::new(
        Binding {
            key: SPACE,
            modifiers: OPTION,
        },
        Binding {
            key: 50,
            modifiers: CONTROL | OPTION,
        },
    );
    action(&mut router, KEY_DOWN, 50, CONTROL | OPTION);
    router.handle(KEY_UP, 50, CONTROL | OPTION, false);
    assert_eq!(
        router.handle(FLAGS_CHANGED, 58, CONTROL, false),
        (false, None)
    );
    assert_eq!(
        action(&mut router, FLAGS_CHANGED, 59, 0).kind,
        ActionKind::Accept
    );
    assert_eq!(
        action(&mut router, KEY_DOWN, SPACE, OPTION).kind,
        ActionKind::Search
    );
    assert_eq!(router.handle(FLAGS_CHANGED, 58, 0, false), (false, None));
}

#[test]
fn escape_and_dismiss_prevent_later_release_from_selecting() {
    let mut router = router();
    let first = action(&mut router, KEY_DOWN, TAB, COMMAND);
    assert_eq!(
        action(&mut router, KEY_DOWN, ESCAPE, COMMAND).kind,
        ActionKind::Cancel
    );
    assert_eq!(router.handle(FLAGS_CHANGED, 55, 0, false), (false, None));
    assert_eq!(router.handle(KEY_UP, ESCAPE, 0, false), (true, None));
    assert_eq!(router.handle(KEY_UP, TAB, 0, false), (true, None));
    let second = action(&mut router, KEY_DOWN, TAB, COMMAND);
    assert_ne!(first.session, second.session);
    router.finish(first.session);
    assert_eq!(
        action(&mut router, FLAGS_CHANGED, 55, 0).session,
        second.session
    );
    router.handle(KEY_UP, TAB, 0, false);
    let third = action(&mut router, KEY_DOWN, TAB, COMMAND);
    router.finish(third.session);
    assert_eq!(router.handle(FLAGS_CHANGED, 55, 0, false), (false, None));
}

#[test]
fn unrelated_keys_pass_through_and_space_respects_search_text_and_ime() {
    let mut router = router();
    for (key, flags) in [(TAB, 0), (0, COMMAND), (SPACE, 0), (TAB, OPTION)] {
        assert_eq!(router.handle(KEY_DOWN, key, flags, false), (false, None));
    }
    assert!(space_changes_mode(PanelMode::Search, true, false));
    assert!(!space_changes_mode(PanelMode::Search, false, false));
    assert!(!space_changes_mode(PanelMode::Search, true, true));
    assert!(space_changes_mode(PanelMode::Switch, false, false));
}

#[test]
fn quick_release_before_discovery_preserves_all_steps_and_commits_once() {
    let mut selection = SwitchSelection::new(1);
    selection.step(1);
    selection.release();
    assert_eq!(selection.take_commit(), None);
    selection.install(4, Some(0));
    assert_eq!(selection.selected(), Some(2));
    assert_eq!(selection.take_commit(), Some(2));
    assert_eq!(selection.take_commit(), None);
    selection.install(9, Some(8));
    assert_eq!(selection.selected(), Some(2));
}

#[test]
fn backwards_and_empty_lists_are_safe() {
    let mut selection = SwitchSelection::new(-1);
    selection.install(0, None);
    assert_eq!(selection.selected(), None);
    selection.install(3, Some(0));
    assert_eq!(selection.selected(), Some(2));
    selection.step(1);
    assert_eq!(selection.selected(), Some(0));
}

#[test]
fn shortcut_mapping_and_conflict_validation_cover_all_selectable_keys() {
    for key in KEYS {
        assert!(
            Shortcut {
                key: (*key).into(),
                ..Shortcut::default()
            }
            .binding()
            .is_ok()
        );
    }
    let config = Config::default();
    assert!(config.switch_shortcut.is_command_tab());
    let invalid = Config {
        shortcut: config.switch_shortcut.clone(),
        ..config.clone()
    };
    assert!(invalid.validate().is_err());
    let invalid = Config {
        switch_shortcut: Shortcut {
            key: "Space".into(),
            ..Shortcut::default()
        },
        ..config
    };
    assert!(invalid.validate().is_err());
}

#[test]
fn old_search_shortcuts_survive_migration_even_when_command_tab_was_used() {
    let raw = r#"{"shortcut":{"control":false,"option":false,"command":true,"key":"Tab"}}"#;
    let config = Config::from_json(raw).unwrap();
    assert!(config.shortcut.is_command_tab());
    assert_ne!(config.shortcut, config.switch_shortcut);
    assert_eq!(
        Config::from_json(&config.to_json().unwrap()).unwrap(),
        config
    );
}

#[test]
fn space_autorepeat_after_local_mode_change_cannot_change_the_router() {
    let mut router = router();
    router.open_search();
    router.enter_switch(COMMAND);
    assert_eq!(router.handle(KEY_DOWN, SPACE, COMMAND, true), (true, None));
    assert_eq!(router.handle(KEY_UP, SPACE, COMMAND, false), (true, None));
    assert_eq!(
        action(&mut router, FLAGS_CHANGED, 55, 0).kind,
        ActionKind::Accept
    );
}

#[test]
fn fast_mode_changes_keep_action_order_and_do_not_confirm_search() {
    let mut router = router();
    let switched = action(&mut router, KEY_DOWN, TAB, COMMAND);
    router.handle(KEY_UP, TAB, COMMAND, false);
    let searched = action(&mut router, KEY_DOWN, SPACE, COMMAND);
    router.handle(KEY_UP, SPACE, COMMAND, false);
    assert_eq!(router.handle(FLAGS_CHANGED, 55, 0, false).1, None);
    let switched_again = action(&mut router, KEY_DOWN, TAB, COMMAND);
    router.handle(KEY_UP, TAB, COMMAND, false);
    let accepted = action(&mut router, FLAGS_CHANGED, 55, 0);
    assert_eq!(switched.session, searched.session);
    assert_eq!(searched.session, switched_again.session);
    assert_eq!(switched_again.session, accepted.session);
    assert_eq!(searched.kind, ActionKind::Search);
    assert_eq!(
        switched_again.kind,
        ActionKind::Switch {
            direction: 1,
            fresh: true
        }
    );
    assert_eq!(accepted.kind, ActionKind::Accept);
}

#[test]
fn failed_activation_can_resume_search_without_cancelling_a_newer_gesture() {
    let mut router = router();
    let first = action(&mut router, KEY_DOWN, TAB, COMMAND);
    router.handle(KEY_UP, TAB, COMMAND, false);
    action(&mut router, FLAGS_CHANGED, 55, 0);
    router.resume_search(first.session);
    assert_eq!(router.handle(FLAGS_CHANGED, 55, COMMAND, false).1, None);
    assert_eq!(router.handle(FLAGS_CHANGED, 55, 0, false).1, None);
    assert_eq!(
        action(&mut router, KEY_DOWN, TAB, CONTROL).kind,
        ActionKind::Cancel
    );
    router.handle(KEY_UP, TAB, CONTROL, false);
    let next = action(&mut router, KEY_DOWN, TAB, COMMAND);
    router.resume_search(first.session);
    let accepted = action(&mut router, FLAGS_CHANGED, 55, 0);
    assert_eq!(accepted.session, next.session);
    assert_eq!(accepted.kind, ActionKind::Accept);
}

#[test]
fn empty_search_to_switch_preserves_selected_window_without_advancing() {
    let mut selection = SwitchSelection::new(0);
    selection.install(5, Some(3));
    assert_eq!(selection.selected(), Some(3));
    assert_eq!(selection.take_commit(), None);
    selection.release();
    assert_eq!(selection.take_commit(), Some(3));
}

#[test]
fn search_cannot_occupy_the_switch_reverse_shortcut() {
    let config = Config {
        shortcut: Shortcut {
            key: "Tab".into(),
            shift: true,
            ..Shortcut::default()
        },
        switch_shortcut: Shortcut {
            key: "Tab".into(),
            ..Shortcut::default()
        },
        ..Config::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn command_space_search_saves_and_routes_without_switching_modes() {
    let config = Config {
        shortcut: Shortcut {
            control: false,
            option: false,
            shift: false,
            command: true,
            key: "Space".into(),
        },
        ..Config::default()
    };
    let saved = config
        .to_json()
        .expect("Command + Space must be accepted for search");
    assert_eq!(Config::from_json(&saved).unwrap(), config);
    let mut router = ShortcutRouter::new(
        config.shortcut.binding().unwrap(),
        config.switch_shortcut.binding().unwrap(),
    );
    assert_eq!(
        action(&mut router, KEY_DOWN, SPACE, COMMAND).kind,
        ActionKind::Search
    );
    assert_eq!(router.handle(KEY_DOWN, SPACE, COMMAND, true), (true, None));
    assert_eq!(router.handle(KEY_UP, SPACE, COMMAND, false), (true, None));
    assert_eq!(router.handle(FLAGS_CHANGED, 0, 0, false), (false, None));
    assert_eq!(
        router.handle(KEY_DOWN, SPACE, 0, false),
        (false, None),
        "plain Space still belongs to the search field"
    );
    assert_eq!(
        action(&mut router, KEY_DOWN, SPACE, COMMAND).kind,
        ActionKind::Cancel
    );
    router.handle(KEY_UP, SPACE, COMMAND, false);
    assert!(matches!(
        action(&mut router, KEY_DOWN, TAB, COMMAND).kind,
        ActionKind::Switch { .. }
    ));
    assert_eq!(
        action(&mut router, KEY_DOWN, SPACE, COMMAND).kind,
        ActionKind::Search
    );
    assert_eq!(
        router.handle(FLAGS_CHANGED, 0, 0, false),
        (false, None),
        "entering search must cancel release-to-switch"
    );

    let invalid_switch = Config {
        switch_shortcut: config.shortcut.clone(),
        shortcut: Shortcut::default(),
        ..config.clone()
    };
    assert!(
        invalid_switch.validate().is_err(),
        "Space remains reserved for mode changes in the switcher"
    );
    let conflicting_app = winlane::config::AppShortcut {
        application: winlane::config::ApplicationTarget {
            name: "Example".into(),
            bundle_id: "com.example.app".into(),
            path: "/Applications/Example.app".into(),
        },
        shortcut: config.shortcut.clone(),
    };
    let conflict = Config {
        app_shortcuts: vec![conflicting_app],
        ..config
    };
    assert!(
        conflict.validate().is_err(),
        "app shortcuts cannot reuse the search binding"
    );
}
