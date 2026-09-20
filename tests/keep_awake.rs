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

#[test]
fn indicator_counts_down_and_disappears_at_deadline() {
    use std::time::{Duration, SystemTime};
    use winlane::features::keep_awake::IndicatorStatus;
    let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    let choice = Choice::Start {
        minutes: Some(30),
        display: false,
    };
    let deadline = choice.deadline(start);
    let status = |elapsed| IndicatorStatus::new(choice, deadline, start + elapsed);
    assert_eq!(status(Duration::ZERO).unwrap().remaining_minutes, Some(30));
    assert_eq!(
        status(Duration::from_secs(59)).unwrap().remaining_minutes,
        Some(30)
    );
    assert_eq!(
        status(Duration::from_secs(60)).unwrap().remaining_minutes,
        Some(29)
    );
    assert_eq!(
        status(Duration::from_millis(1799999))
            .unwrap()
            .remaining_minutes,
        Some(1)
    );
    assert!(status(Duration::from_secs(1800)).is_none());
    assert!(status(Duration::from_secs(1801)).is_none());
    assert!(IndicatorStatus::new(Choice::Stop, None, start).is_none());
    let forever = IndicatorStatus::new(
        Choice::Start {
            minutes: None,
            display: true,
        },
        None,
        start,
    )
    .unwrap();
    assert!(forever.display);
    assert!(forever.remaining_minutes.is_none());
    assert!(forever.label().contains('☕'));
    assert_ne!(forever.label(), status(Duration::ZERO).unwrap().label());
}

#[test]
fn indicator_placement_avoids_input_indicator_and_respects_safe_area() {
    use winlane::core::displays::Rect;
    use winlane::features::keep_awake::indicator_frame;
    for safe in [
        Rect {
            x: 0.0,
            y: 40.0,
            width: 1440.0,
            height: 830.0,
        },
        Rect {
            x: -1920.0,
            y: -160.0,
            width: 1920.0,
            height: 1010.0,
        },
        Rect {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 16.0,
        },
    ] {
        let frame = indicator_frame(safe, 1000.0, &[]);
        assert!(frame.x >= safe.x && frame.y >= safe.y);
        assert!(frame.x + frame.width <= safe.x + safe.width);
        assert!(frame.y + frame.height <= safe.y + safe.height);
    }
    let safe = Rect {
        x: 0.0,
        y: 40.0,
        width: 1440.0,
        height: 830.0,
    };
    let original = indicator_frame(safe, 120.0, &[]);
    let moved = indicator_frame(safe, 120.0, &[original]);
    assert!(moved.y + moved.height < original.y);
    assert_eq!(moved.x, original.x);
    assert_eq!(indicator_frame(safe, 120.0, &[]), original);
    let tall = Rect {
        y: safe.y,
        height: safe.height,
        ..original
    };
    let left = indicator_frame(safe, 120.0, &[tall]);
    assert!(left.x + left.width < tall.x);
}
