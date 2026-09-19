use winlane::config::{Config, visible_matches};
use winlane::discovery::{
    AX_CANNOT_COMPLETE, AX_NO_VALUE, FocusRead, finish_application_scan, merge_window_sources,
    normal_window_surface, read_focused_window, read_published_windows, read_with_retry,
    remote_window_token, switchable_window,
};
use winlane::search::WindowInfo;

#[test]
fn backing_surfaces_must_be_normal_visible_sized_windows() {
    assert!(normal_window_surface(0, 1.0, 1200.0, 800.0));
    assert!(normal_window_surface(0, 1.0, 390.0, 250.0));
    for values in [
        (3, 1.0, 1200.0, 800.0),
        (0, 0.0, 1200.0, 800.0),
        (0, 1.0, 1.0, 1.0),
        (0, 1.0, 0.0, 800.0),
        (0, 1.0, 800.0, f64::NAN),
    ] {
        assert!(!normal_window_surface(
            values.0, values.1, values.2, values.3
        ));
    }
}

#[test]
fn remote_window_identity_preserves_process_and_full_element_id() {
    let id = u64::from(u32::MAX) + 73;
    let token = remote_window_token(421, id);
    assert_eq!(i32::from_ne_bytes(token[..4].try_into().unwrap()), 421);
    assert_eq!(&token[4..8], &[0; 4]);
    assert_eq!(
        u32::from_ne_bytes(token[8..12].try_into().unwrap()),
        0x636f636f
    );
    assert_eq!(u64::from_ne_bytes(token[12..].try_into().unwrap()), id);
    assert_ne!(token, remote_window_token(422, id));
    assert_ne!(token, remote_window_token(421, id + 1));
}

#[test]
fn other_space_windows_survive_empty_or_unavailable_current_space_list() {
    for windows in [Ok(vec![]), Err(AX_CANNOT_COMPLETE)] {
        let result = read_published_windows(|attribute| match attribute {
            "AXWindows" => windows.clone(),
            "AXChildren" => Ok(vec![]),
            "AXFocusedWindow" | "AXMainWindow" => Ok(vec![42]),
            _ => panic!("unexpected attribute {attribute}"),
        });
        assert_eq!(result, Ok(vec![42]));
    }
}

#[test]
fn published_sources_keep_distinct_windows_and_deduplicate_the_same_window() {
    let result = read_published_windows(|attribute| {
        Ok(match attribute {
            "AXWindows" => vec![11, 12],
            "AXChildren" => vec![12],
            "AXFocusedWindow" => vec![13],
            "AXMainWindow" => vec![11],
            _ => panic!("unexpected attribute {attribute}"),
        })
    });
    assert_eq!(result, Ok(vec![11, 12, 13]));
}

#[test]
fn a_slow_application_gets_one_longer_read_attempt() {
    let mut timeouts = Vec::new();
    let result = read_with_retry(|timeout| {
        timeouts.push(timeout);
        if timeout < 0.5 {
            Err(AX_CANNOT_COMPLETE)
        } else {
            Ok("window list")
        }
    });
    assert_eq!(result, Ok("window list"));
    assert_eq!(timeouts.len(), 2);
    assert!(timeouts[1] > timeouts[0]);
}

#[test]
fn a_slow_focused_window_does_not_block_the_shortcut_but_can_resolve_in_background() {
    for (policy, expected, calls) in [
        (FocusRead::Immediate, Err(AX_CANNOT_COMPLETE), 1),
        (FocusRead::Background, Ok(42), 2),
    ] {
        let mut timeouts = Vec::new();
        let result = read_focused_window(policy, |timeout| {
            timeouts.push(timeout);
            if timeout < 0.3 {
                Err(AX_CANNOT_COMPLETE)
            } else {
                Ok(42)
            }
        });
        assert_eq!(result, expected);
        assert_eq!(timeouts.len(), calls);
        if calls == 1 {
            assert_eq!(timeouts, [0.02]);
        } else {
            assert!(timeouts[0] > 0.02);
            assert!(timeouts[1] > timeouts[0]);
            assert!(timeouts.iter().sum::<f32>() <= 1.0);
        }
    }
}

#[test]
fn a_window_exposed_after_activation_gets_one_background_retry() {
    let mut calls = 0;
    let result = read_focused_window(FocusRead::Background, |_| {
        calls += 1;
        if calls == 1 { Err(AX_NO_VALUE) } else { Ok(42) }
    });
    assert_eq!(result, Ok(42));
    assert_eq!(calls, 2);
}

#[test]
fn focused_window_retries_are_bounded_and_only_for_transient_background_failures() {
    for error in [AX_CANNOT_COMPLETE, AX_NO_VALUE, -25202, -25205, -25211] {
        for policy in [FocusRead::Immediate, FocusRead::Background] {
            let mut calls = 0;
            let result: Result<(), _> = read_focused_window(policy, |_| {
                calls += 1;
                Err(error)
            });
            assert_eq!(result, Err(error));
            let retry = matches!(policy, FocusRead::Background)
                && matches!(error, AX_CANNOT_COMPLETE | AX_NO_VALUE);
            assert_eq!(calls, if retry { 2 } else { 1 });
        }
    }
    for policy in [FocusRead::Immediate, FocusRead::Background] {
        let mut calls = 0;
        assert_eq!(
            read_focused_window(policy, |_| {
                calls += 1;
                Ok(42)
            }),
            Ok(42)
        );
        assert_eq!(calls, 1);
    }
}

#[test]
fn alternate_window_source_recovers_missing_projects_and_deduplicates_handles() {
    assert_eq!(
        merge_window_sources(Ok(vec![11, 12]), Ok(vec![12, 13])),
        Ok(vec![11, 12, 13])
    );
    assert_eq!(
        merge_window_sources(Err(AX_CANNOT_COMPLETE), Ok(vec![11, 12, 13])),
        Ok(vec![11, 12, 13])
    );
    assert_eq!(
        merge_window_sources(Ok(vec![11, 12, 13]), Err(-25205)),
        Ok(vec![11, 12, 13])
    );
    assert_eq!(
        merge_window_sources(Ok(vec![11, 11]), Ok(vec![])),
        Ok(vec![11])
    );
    assert_eq!(
        merge_window_sources::<i32>(Err(AX_CANNOT_COMPLETE), Err(-25205)),
        Err(AX_CANNOT_COMPLETE)
    );
}

#[test]
fn retry_is_bounded_and_does_not_retry_unsupported_attributes() {
    for (error, expected_calls) in [(AX_CANNOT_COMPLETE, 2), (-25205, 1), (-25202, 1)] {
        let mut calls = 0;
        let result: Result<(), _> = read_with_retry(|_| {
            calls += 1;
            Err(error)
        });
        assert_eq!(result, Err(error));
        assert_eq!(calls, expected_calls);
    }
}

#[test]
fn unreadable_and_windowless_apps_do_not_create_candidates() {
    for result in [Ok(Vec::new()), Err(AX_CANNOT_COMPLETE)] {
        assert!(finish_application_scan(result).is_empty());
    }
}

#[test]
fn minimized_windows_remain_candidates_without_app_duplicates() {
    let window = WindowInfo {
        id: 7,
        pid: 42,
        app: "Code".into(),
        title: "main.rs".into(),
        minimized: true,
    };
    let items = finish_application_scan(Ok(vec![window]));
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, 7);
    assert!(items[0].minimized);
    let config = Config {
        include_minimized: false,
        ..Config::default()
    };
    assert!(visible_matches(&items, "", None, &config, None, &[], 0).is_empty());
}

#[test]
fn only_standard_windows_are_candidates() {
    assert!(switchable_window(
        Some("AXWindow"),
        Some("AXStandardWindow")
    ));
    for subrole in [
        None,
        Some(""),
        Some("AXUnknown"),
        Some("AXFloatingWindow"),
        Some("AXSystemFloatingWindow"),
        Some("AXDialog"),
        Some("AXSystemDialog"),
    ] {
        assert!(!switchable_window(Some("AXWindow"), subrole), "{subrole:?}");
    }
    for role in [
        None,
        Some("AXApplication"),
        Some("AXTab"),
        Some("AXSheet"),
        Some("AXMenuItem"),
    ] {
        assert!(
            !switchable_window(role, Some("AXStandardWindow")),
            "{role:?}"
        );
    }
}

#[test]
fn code_project_windows_keep_separate_identities_through_search_and_scope() {
    let projects = [
        (11, "lib.rs — compiler"),
        (12, "main.rs — node"),
        (13, "settings.rs — winlane"),
    ];
    let windows = projects
        .into_iter()
        .map(|(id, title)| WindowInfo {
            id,
            pid: 42,
            app: "Code".into(),
            title: title.into(),
            minimized: id == 12,
        })
        .collect();
    let items = finish_application_scan(Ok(windows));
    let config = Config::default();
    let matches = visible_matches(&items, "code", None, &config, Some(42), &[], 0);
    let mut ids: Vec<_> = matches.into_iter().map(|index| items[index].id).collect();
    ids.sort();
    assert_eq!(ids, vec![11, 12, 13]);
    for (query, expected) in [
        ("compiler", &[11][..]),
        ("node", &[12, 11, 13][..]),
        ("winlane", &[13][..]),
    ] {
        let matches = visible_matches(&items, query, None, &config, None, &[], 0);
        let ids: Vec<_> = matches.into_iter().map(|index| items[index].id).collect();
        assert_eq!(
            ids, expected,
            "exact projects precede app-name typo matches"
        );
    }
}

#[test]
fn distinct_windows_with_identical_titles_are_not_collapsed() {
    let items = finish_application_scan(Ok([101, 102]
        .into_iter()
        .map(|id| WindowInfo {
            id,
            pid: 42,
            app: "Google Chrome".into(),
            title: "New Tab".into(),
            minimized: false,
        })
        .collect()));
    assert_eq!(items.len(), 2);
    let matches = visible_matches(&items, "chrome", None, &Config::default(), Some(42), &[], 0);
    assert_eq!(matches.len(), 2);
    assert_ne!(items[matches[0]].id, items[matches[1]].id);
}
