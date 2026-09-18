use winlane::config::Config;
use winlane::input_method::{InputGate, InputMethod, InputSession};

#[test]
fn missing_input_policy_defaults_to_english_and_saved_choices_survive_restart() {
    assert_eq!(
        Config::from_json("{}").unwrap().input_method,
        InputMethod::English
    );
    for input_method in [
        InputMethod::Current,
        InputMethod::English,
        InputMethod::Chinese,
        InputMethod::LastUsed,
    ] {
        let config = Config {
            input_method,
            ..Config::default()
        };
        assert_eq!(
            Config::from_json(&config.to_json().unwrap()).unwrap(),
            config
        );
    }
    assert!(Config::from_json(r#"{"input_method":"invalid"}"#).is_err());
}

#[test]
fn remembers_manual_changes_and_restores_original_source_only_once() {
    let mut session = InputSession::default();
    session.prepare(Some("Chinese".into()), InputMethod::English);
    assert!(!session.observe(Some("Unfocused app".into())));
    session.focused = true;
    assert!(session.observe(Some("English".into())));
    assert!(!session.observe(Some("English".into())));
    assert!(session.observe(Some("Japanese".into())));
    assert_eq!(session.selected.as_deref(), Some("Japanese"));
    assert!(!session.observe(None));
    assert_eq!(session.finish(Some("Japanese")), Some("Chinese".into()));
    assert_eq!(session.finish(Some("Japanese")), None);
    assert!(!session.observe(Some("Other app".into())));
}

#[test]
fn follow_current_does_not_undo_manual_switches() {
    let mut session = InputSession::default();
    session.prepare(Some("Chinese".into()), InputMethod::Current);
    session.focused = true;
    session.observe(Some("English".into()));
    assert_eq!(session.finish(Some("English")), None);
}

#[test]
fn lost_focus_or_changed_destination_source_must_not_be_overridden() {
    for current in [None, Some("Destination choice")] {
        let mut session = InputSession::default();
        session.prepare(Some("Chinese".into()), InputMethod::LastUsed);
        session.focused = true;
        session.observe(Some("English".into()));
        assert_eq!(session.finish(current), None);
    }
}

#[test]
fn reentering_search_starts_a_new_policy_session_and_unavailable_sources_are_unchanged() {
    let mut session = InputSession::default();
    session.prepare(Some("English".into()), InputMethod::Chinese);
    session.focused = true;
    session.observe(Some("English".into()));
    assert_eq!(session.finish(Some("English")), None);
    session.prepare(Some("English".into()), InputMethod::Chinese);
    assert!(!session.focused);
    assert!(session.selected.is_none());
}

#[test]
fn early_typing_waits_for_the_target_source_and_keeps_editing_order() {
    let mut gate = InputGate::default();
    gate.begin(Some("English".into()));
    for key in ["r", "u", "Backspace", "s", "Return"] {
        gate.push(key);
    }
    assert_eq!(gate.finish(Some("Chinese"), false), None);
    assert_eq!(gate.finish(None, false), None);
    assert_eq!(
        gate.finish(Some("English"), false),
        Some(vec!["r", "u", "Backspace", "s", "Return"])
    );
    assert!(gate.target().is_none());
    assert_eq!(
        gate.finish(Some("English"), false),
        Some(vec![]),
        "notifications must not replay twice"
    );
    assert_eq!(
        gate.finish(Some("Chinese"), false),
        Some(vec![]),
        "manual changes after startup remain allowed"
    );
}

#[test]
fn cancelled_input_never_leaks_into_a_new_search() {
    let mut gate = InputGate::default();
    gate.begin(Some("English".into()));
    gate.push("old search");
    gate.begin(None); // Escape or focus loss cancels the queued keys.
    assert_eq!(gate.finish(Some("English"), false), Some(vec![]));
    gate.begin(Some("Chinese".into()));
    gate.push("new search");
    assert_eq!(gate.finish(Some("English"), false), None);
    assert_eq!(
        gate.finish(Some("Chinese"), false),
        Some(vec!["new search"])
    );
}

#[test]
fn already_selected_and_follow_current_sources_do_not_wait() {
    let mut gate: InputGate<char> = InputGate::default();
    for current in ["English", "Chinese", "Saved input source"] {
        gate.begin(Some(current.into()));
        assert_eq!(gate.finish(Some(current), false), Some(vec![]));
    }
    gate.begin(None); // Current policy, or no available source for the policy.
    assert_eq!(gate.finish(Some("Chinese"), false), Some(vec![]));
}

#[test]
fn language_and_last_used_policies_wait_for_their_own_target_and_restore_the_source() {
    for (policy, previous, target) in [
        (InputMethod::English, "Chinese", "English"),
        (InputMethod::Chinese, "English", "Chinese"),
        (InputMethod::LastUsed, "Chinese", "Japanese"),
    ] {
        let mut session = InputSession::default();
        session.prepare(Some(previous.into()), policy);
        let mut gate = InputGate::default();
        gate.begin(Some(target.into()));
        for key in ["n", "i", "Space"] {
            gate.push(key);
        }
        assert_eq!(gate.finish(Some(previous), false), None);
        assert_eq!(gate.finish(None, false), None);
        session.focused = true;
        assert!(session.observe(Some(target.into())));
        assert_eq!(
            gate.finish(Some(target), false),
            Some(vec!["n", "i", "Space"])
        );
        assert_eq!(session.finish(Some(target)), Some(previous.into()));
    }
}

#[test]
fn failed_source_selection_has_a_bounded_wait_without_losing_keys() {
    let mut gate = InputGate::default();
    gate.begin(Some("Unavailable".into()));
    gate.push("kept");
    assert_eq!(gate.finish(Some("English"), false), None);
    assert_eq!(gate.finish(Some("English"), true), Some(vec!["kept"]));
    assert!(gate.target().is_none());
    assert_eq!(gate.finish(Some("Unavailable"), false), Some(vec![]));
}
