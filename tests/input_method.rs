use winlane::config::Config;
use winlane::input_method::{InputMethod, InputSession};

#[test]
fn old_settings_keep_current_input_and_all_choices_survive_restart() {
    assert_eq!(
        Config::from_json("{}").unwrap().input_method,
        InputMethod::Current
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
