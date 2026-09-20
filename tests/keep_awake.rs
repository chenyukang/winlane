use std::time::{Duration, SystemTime};
use winlane::{
    core::commands::{CommandId, matching_commands},
    features::keep_awake::{CHOICES, Choice, matching},
};

#[test]
fn awake_command_and_duration_choices_are_searchable() {
    for query in ["keep-awake", "caffeine", "防休眠"] {
        assert_eq!(matching_commands(query), [CommandId::KeepAwake]);
    }
    assert_eq!(matching("").len(), 7);
    assert_eq!(matching("30").len(), 2);
    assert!(matching("invalid duration").is_empty());
}

#[test]
fn timed_and_indefinite_sessions_have_distinct_deadlines() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    for choice in CHOICES {
        match choice {
            Choice::Start {
                minutes: Some(m), ..
            } => assert_eq!(
                choice.deadline(now),
                Some(now + Duration::from_secs(u64::from(m) * 60))
            ),
            _ => assert_eq!(choice.deadline(now), None),
        }
    }
}
