use super::*;

pub(super) fn verify_meeting(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.catalog_checked.set(Some(Instant::now()));
    delegate.sync_displays();
    // Inject today's meetings directly so the test never touches EventKit or the calendar.
    state
        .meeting_results
        .replace(winlane::features::meeting::Meetings {
            items: vec![
                winlane::features::meeting::Meeting {
                    id: "1".into(),
                    title: "Standup".into(),
                    start_label: "09:30".into(),
                    day: String::new(),
                    relative: "Starting in 20 minutes".into(),
                    link: Some("https://meet.google.com/aaa-bbb".into()),
                    calendar: "Work".into(),
                    location: String::new(),
                },
                winlane::features::meeting::Meeting {
                    id: "2".into(),
                    title: "Focus".into(),
                    start_label: "11:00".into(),
                    day: "Wed, Sep 24".into(),
                    relative: "Completed".into(),
                    link: None,
                    calendar: "Personal".into(),
                    location: "Desk".into(),
                },
            ],
            error: None,
            access_denied: false,
        });
    // Enter the scope without scheduling a background EventKit refresh.
    state.mode.set(Some(PanelMode::Search));
    state.search_scope.set(Some(SearchScope::Meeting));
    assert!(delegate.searching_meeting());
    state.query.borrow_mut().clear();
    delegate.filter_preserving(None);
    assert_eq!(delegate.match_count(), 2);
    assert_eq!(delegate.selected_meeting().unwrap().id, "1");
    for ui in delegate.panels() {
        let rows = ui.rows.borrow();
        assert_eq!(rows[0].app.stringValue().to_string(), "09:30");
        let title = rows[0].title.stringValue().to_string();
        assert!(title.contains("Standup") && title.contains("meet.google.com"));
        assert!(title.contains("Starting in 20 minutes"));
        assert_eq!(rows[0].alias.stringValue().to_string(), "↗");
        assert!(rows[0].icon.image().is_some());
        // A meeting on another day shows its weekday and date.
        let second = rows[1].title.stringValue().to_string();
        assert!(second.contains("Wed, Sep 24") && second.contains("Focus"));
        assert!(!ui.panel.isVisible());
    }

    // The meeting footer stays visible even when the usage-hints toggle is off.
    state.config.borrow_mut().show_usage_hints = false;
    delegate.filter_preserving(None);
    assert!(delegate.panels().iter().all(|ui| !ui.footer.isHidden()));
    state.config.borrow_mut().show_usage_hints = true;
    delegate.filter_preserving(None);

    // > and < move between days; other characters keep filtering.
    let key = |chars: &str| {
        let text = NSString::from_str(chars);
        NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::KeyDown, NSPoint::ZERO, NSEventModifierFlags::Shift, 0.0, 0,
            None, &text, &text, false, 0,
        ).unwrap()
    };
    assert_eq!(delegate.meeting_day_key(&key(">"), false), Some(1));
    assert_eq!(delegate.meeting_day_key(&key("<"), false), Some(-1));
    assert_eq!(delegate.meeting_day_key(&key("z"), false), None);
    assert_eq!(delegate.meeting_day_key(&key(">"), true), None);

    // Filtering narrows the list, and a meeting without a link reports it on Enter.
    state.query.replace("focus".into());
    delegate.filter_preserving(None);
    assert_eq!(delegate.match_count(), 1);
    assert_eq!(delegate.selected_meeting().unwrap().id, "2");
    delegate.submit_meeting();
    assert!(state.meeting_results.borrow().error.is_some());
    assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));

    // Leaving the scope clears the injected rows.
    delegate.end_session();
    assert!(state.meeting_matches.borrow().is_empty());
    assert!(!delegate.searching_meeting());
    println!(
        "Meeting checks passed: scope entry, injected list, filtering, link display, day navigation keys, missing-link handling, and cleanup; no calendar accessed."
    );
}
