use winlane::features::meeting::{
    Meeting, day_label, extract_link, is_web_url, matching, relative_label, status_label,
};

fn meeting(id: &str, title: &str, calendar: &str, location: &str, link: Option<&str>) -> Meeting {
    Meeting {
        id: id.into(),
        title: title.into(),
        start_label: "10:00".into(),
        day: String::new(),
        relative: String::new(),
        link: link.map(str::to_string),
        calendar: calendar.into(),
        location: location.into(),
    }
}

#[test]
fn status_label_marks_completed_in_progress_and_countdown() {
    // Ended meetings read the same regardless of how long ago.
    assert_eq!(status_label(-600, -60), status_label(-10, -1));
    // In progress: started, not yet ended.
    assert_eq!(status_label(-30, 600), status_label(-1, 100));
    assert_ne!(status_label(-1, -1), status_label(-1, 100));
    // Upcoming reuses the countdown.
    assert_eq!(status_label(1200, 3600), relative_label(1200));
}

#[test]
fn relative_label_scales_from_minutes_to_days() {
    assert!(relative_label(20 * 60).contains("20"));
    assert!(relative_label(2 * 3600).contains('2'));
    assert!(relative_label(26 * 3600).contains('1'));
    // A far-off start rounds to whole days.
    assert!(relative_label(50 * 3600).contains('2'));
}

#[test]
fn day_label_names_nearby_days() {
    assert_ne!(day_label(0), day_label(1));
    assert_ne!(day_label(0), day_label(-1));
    assert!(day_label(3).contains('3'));
    assert!(day_label(-4).contains('4'));
}

#[test]
fn extracts_links_from_url_then_location_then_notes() {
    // An explicit web URL field wins.
    assert_eq!(
        extract_link(Some("https://zoom.us/j/123"), "", ""),
        Some("https://zoom.us/j/123".into())
    );
    // A non-web URL field is ignored; a provider link in the location is used instead.
    assert_eq!(
        extract_link(
            Some("message:xyz"),
            "Join https://meet.google.com/abc-def now",
            ""
        ),
        Some("https://meet.google.com/abc-def".into())
    );
    // Notes are searched after the location, and trailing punctuation is trimmed.
    assert_eq!(
        extract_link(
            None,
            "Room 5",
            "Teams link: https://teams.microsoft.com/l/xyz)"
        ),
        Some("https://teams.microsoft.com/l/xyz".into())
    );
    // A non-provider URL is not treated as a meeting link.
    assert_eq!(
        extract_link(None, "", "notes at https://example.com/plan"),
        None
    );
    assert_eq!(extract_link(None, "", ""), None);
}

#[test]
fn is_web_url_requires_http_scheme_without_spaces() {
    assert!(is_web_url("https://zoom.us/j/1"));
    assert!(is_web_url("http://a.bc"));
    assert!(!is_web_url("zoom.us/j/1"));
    assert!(!is_web_url("https://has space.com"));
    assert!(!is_web_url("mailto:x@y.com"));
}

#[test]
fn matching_filters_by_title_calendar_location_and_link() {
    let items = vec![
        meeting(
            "1",
            "Standup",
            "Work",
            "",
            Some("https://meet.google.com/aaa"),
        ),
        meeting("2", "Lunch", "Personal", "Cafe", None),
        meeting(
            "3",
            "Design review",
            "Work",
            "",
            Some("https://zoom.us/j/7"),
        ),
    ];
    let ids = |query: &str| {
        matching(&items, query)
            .into_iter()
            .map(|item| item.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(""), ["1", "2", "3"]);
    assert_eq!(ids("stand"), ["1"]);
    assert_eq!(ids("zoom"), ["3"]);
    assert_eq!(ids("personal"), ["2"]);
    assert_eq!(ids("cafe"), ["2"]);
    assert!(ids("zzz").is_empty());
}
