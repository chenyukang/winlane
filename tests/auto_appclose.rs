use winlane::core::config::{ApplicationTarget, Config};
use winlane::features::auto_appclose::{
    MAX_IDLE_MINUTES, Planner, Reason, Rule, Settings, Snapshot, Window,
};

fn rule(bundle: &str, max_windows: u16) -> Rule {
    Rule {
        application: ApplicationTarget {
            bundle_id: bundle.into(),
            name: "Editor".into(),
            path: "/Applications/Editor.app".into(),
        },
        max_windows: Some(max_windows),
        max_idle_minutes: None,
    }
}

fn idle_rule(bundle: &str, max_idle_minutes: u16) -> Rule {
    Rule {
        max_windows: None,
        max_idle_minutes: Some(max_idle_minutes),
        ..rule(bundle, 1)
    }
}
fn settings() -> Settings {
    Settings {
        enabled: true,
        rules: vec![rule("test.editor", 3)],
        ..Settings::default()
    }
}
fn snapshot(ids: &[u64]) -> Vec<Snapshot> {
    snapshot_with_focus(ids, None)
}

fn snapshot_with_focus(ids: &[u64], focused: Option<u64>) -> Vec<Snapshot> {
    vec![Snapshot {
        bundle_id: "test.editor".into(),
        windows: ids
            .iter()
            .map(|&id| Window {
                id,
                pid: 10,
                server_id: Some(id as u32),
                protected: false,
            })
            .collect(),
        focused,
    }]
}

#[test]
fn defaults_are_off_and_rules_survive_disabling_and_restart() {
    assert_eq!(
        Config::from_json("{}").unwrap().auto_appclose,
        Settings::default()
    );
    let mut config = Config {
        auto_appclose: settings(),
        ..Config::default()
    };
    config.auto_appclose.enabled = false;
    let loaded = Config::from_json(&config.to_json().unwrap()).unwrap();
    assert_eq!(loaded, config);
    assert_eq!(loaded.auto_appclose.rules[0].max_windows, Some(3));
    assert_eq!(loaded.auto_appclose.interval_secs, 10);
}

#[test]
fn renamed_settings_migrate_without_losing_rules_or_intervals() {
    for enabled in [false, true] {
        let mut expected = Config {
            auto_appclose: settings(),
            ..Config::default()
        };
        expected.auto_appclose.enabled = enabled;
        expected.auto_appclose.interval_secs = 30;
        expected.auto_appclose.rules.push(rule("test.other", 2));
        let mut old = serde_json::to_value(&expected).unwrap();
        let settings = old
            .as_object_mut()
            .unwrap()
            .remove("auto_appclose")
            .unwrap();
        old["auto_cleanup"] = settings;

        let loaded = Config::from_json(&old.to_string()).unwrap();
        assert_eq!(loaded, expected);
        let saved: serde_json::Value = serde_json::from_str(&loaded.to_json().unwrap()).unwrap();
        assert!(saved.get("auto_cleanup").is_none());
        assert_eq!(saved["auto_appclose"], old["auto_cleanup"]);
        assert_eq!(Config::from_json(&saved.to_string()).unwrap(), expected);
    }
}

#[test]
fn renamed_settings_take_precedence_over_old_key_including_disabled_state() {
    let expected = Config::default();
    let mut both = serde_json::to_value(&expected).unwrap();
    both["auto_cleanup"] = serde_json::to_value(settings()).unwrap();
    assert_eq!(Config::from_json(&both.to_string()).unwrap(), expected);
}

#[test]
fn invalid_legacy_settings_are_rejected_instead_of_reset() {
    let mut old = serde_json::json!({"auto_cleanup": settings()});
    old["auto_cleanup"]["interval_secs"] = 0.into();
    assert!(Config::from_json(&old.to_string()).is_err());
}

#[test]
fn legacy_rules_get_ten_seconds_and_custom_intervals_persist() {
    let config = Config {
        auto_appclose: settings(),
        ..Config::default()
    };
    let mut old = serde_json::to_value(&config).unwrap();
    let mut legacy_settings = old
        .as_object_mut()
        .unwrap()
        .remove("auto_appclose")
        .unwrap();
    legacy_settings
        .as_object_mut()
        .unwrap()
        .remove("interval_secs");
    old["auto_cleanup"] = legacy_settings;
    let loaded = Config::from_json(&old.to_string()).unwrap();
    assert_eq!(loaded, config);
    assert_eq!(loaded.auto_appclose.interval().as_secs(), 10);
    assert_eq!(loaded.auto_appclose.grace_period_ms(), 20_000);
    for interval_secs in [1, 10, 30, 3600] {
        let mut config = config.clone();
        config.auto_appclose.interval_secs = interval_secs;
        assert_eq!(
            Config::from_json(&config.to_json().unwrap()).unwrap(),
            config
        );
    }
    for interval_secs in [0, 3601, u16::MAX] {
        let mut config = config.clone();
        config.auto_appclose.interval_secs = interval_secs;
        assert!(config.validate().is_err());
        assert!(Config::from_json(&serde_json::to_string(&config).unwrap()).is_err());
    }
}

#[test]
fn new_window_protection_is_exactly_twice_the_configured_interval() {
    let mut planner = Planner::default();
    let windows = snapshot(&[1, 2, 3, 4]);
    let discovered = 123_456;
    planner.observe(&windows, discovered);
    for interval_secs in [1, 10, 30, 3600] {
        let settings = Settings {
            interval_secs,
            ..settings()
        };
        let ready = discovered + u64::from(interval_secs) * 2_000;
        assert!(
            planner
                .candidate(&settings, &windows, &[], ready - 1)
                .is_none()
        );
        assert!(planner.candidate(&settings, &windows, &[], ready).is_some());
    }
}

#[test]
fn validates_limits_duplicates_and_self_appclose() {
    let mut s = settings();
    for limit in [0, 101, u16::MAX] {
        s.rules[0].max_windows = Some(limit);
        assert!(s.validate().is_err());
    }
    for limit in [1, 3, 100] {
        s.rules[0].max_windows = Some(limit);
        assert!(s.validate().is_ok());
    }
    // Either limit may stand alone, but a rule with neither would do nothing.
    s.rules[0].max_windows = None;
    assert!(s.validate().is_err());
    s.rules[0].max_idle_minutes = Some(240);
    assert!(s.validate().is_ok());
    s.rules[0].max_idle_minutes = Some(0);
    assert!(s.validate().is_err());
    s.rules[0].max_idle_minutes = Some(MAX_IDLE_MINUTES + 1);
    assert!(s.validate().is_err());
    for minutes in [1, 240, MAX_IDLE_MINUTES] {
        s.rules[0].max_idle_minutes = Some(minutes);
        assert!(s.validate().is_ok());
    }
    s.rules[0].max_idle_minutes = None;
    s.rules[0].max_windows = Some(3);
    s.rules.push(s.rules[0].clone());
    assert!(s.validate().is_err());
    s.rules = vec![rule("app.windowlane.desktop", 3)];
    assert!(s.validate().is_err());
    s.rules = (0..65).map(|i| rule(&format!("test.app{i}"), 3)).collect();
    assert!(s.validate().is_err());
}

#[test]
fn closes_oldest_by_mru_not_inventory_order_and_honors_global_switch() {
    let mut p = Planner::default();
    let s = snapshot(&[2, 4, 1, 3]);
    p.observe(&s, 0);
    assert!(p.candidate(&settings(), &s, &[1, 3, 4, 2], 19999).is_none());
    assert_eq!(
        p.candidate(&settings(), &s, &[1, 3, 4, 2], 20000)
            .unwrap()
            .window
            .id,
        2
    );
    let mut off = settings();
    off.enabled = false;
    assert!(p.candidate(&off, &s, &[1, 3, 4, 2], 20000).is_none());
    assert!(
        p.candidate(&settings(), &snapshot(&[1, 2, 3]), &[1, 2, 3], 20000)
            .is_none()
    );
}

#[test]
fn protects_focused_modified_new_and_unidentifiable_windows() {
    let mut p = Planner::default();
    let mut s = snapshot(&[1, 2, 3, 4]);
    s[0].windows[3].protected = true;
    s[0].windows[2].server_id = None;
    p.observe(&s, 0);
    s[0].windows.push(Window {
        id: 5,
        pid: 10,
        server_id: Some(5),
        protected: false,
    });
    p.observe(&s, 20000);
    assert_eq!(
        p.candidate(&settings(), &s, &[1, 2, 3, 4], 20000)
            .unwrap()
            .window
            .id,
        2
    );
    assert_eq!(
        p.candidate(&settings(), &s, &[1, 2, 3, 4], 40000)
            .unwrap()
            .window
            .id,
        5
    );
}

#[test]
fn pending_close_blocks_newer_windows_until_confirmed_and_other_apps_continue() {
    let mut p = Planner::default();
    let s = snapshot(&[1, 2, 3, 4]);
    p.observe(&s, 0);
    let target = p.candidate(&settings(), &s, &[1, 2, 3, 4], 20000).unwrap();
    p.attempted(target.clone());
    assert!(
        p.candidate(&settings(), &s, &[1, 2, 3, 4], 9919999)
            .is_none()
    );
    assert!(p.confirm_closed(&[99]).is_empty());
    assert_eq!(p.pending().count(), 1);
    let mut rules = settings();
    rules.rules.push(rule("test.other", 1));
    let mut both = s.clone();
    let mut other = snapshot(&[6, 7]).remove(0);
    other.bundle_id = "test.other".into();
    both.push(other);
    p.observe(&both, 20000);
    assert_eq!(
        p.candidate(&rules, &both, &[], 40000).unwrap().bundle_id,
        "test.other"
    );
    assert_eq!(p.confirm_closed(&[4]), vec![target]);
    assert!(
        p.candidate(&settings(), &snapshot(&[1, 2, 3]), &[], 40000)
            .is_none()
    );
}

#[test]
fn missing_or_failed_app_scan_never_closes_and_reappearance_gets_grace() {
    let mut p = Planner::default();
    p.observe(&snapshot(&[1, 2, 3, 4]), 0);
    p.observe(&[], 20000);
    assert!(p.candidate(&settings(), &[], &[], 40000).is_none());
    let s = snapshot(&[1, 2, 3, 4]);
    p.observe(&s, 40000);
    assert!(p.candidate(&settings(), &s, &[], 40000).is_none());
}

#[test]
fn same_bundle_across_processes_counts_as_one_rule_and_latest_recency_wins() {
    let mut p = Planner::default();
    let mut s = snapshot(&[1, 2, 3, 4]);
    s[0].windows[3].pid = 20;
    p.observe(&s, 0);
    assert_eq!(
        p.candidate(&settings(), &s, &[1, 2, 3, 4], 20000)
            .unwrap()
            .window
            .pid,
        20
    );
    assert_eq!(
        p.candidate(&settings(), &s, &[4, 1, 2, 3], 20000)
            .unwrap()
            .window
            .id,
        3
    );
}

#[test]
fn closes_a_window_only_after_it_has_been_idle_for_the_configured_time() {
    let mut planner = Planner::default();
    let windows = snapshot(&[1, 2]);
    let start = 0;
    planner.observe(&windows, start);
    let settings = Settings {
        enabled: true,
        rules: vec![idle_rule("test.editor", 60)],
        ..Settings::default()
    };
    // Both windows were open when tracking started, so both count from then,
    // not from the epoch, and neither is closed before its hour is up.
    assert!(
        planner
            .candidate(&settings, &windows, &[1, 2], 59 * 60_000)
            .is_none()
    );
    let target = planner
        .candidate(&settings, &windows, &[1, 2], 60 * 60_000)
        .unwrap();
    assert_eq!(target.reason, Reason::Idle);
    assert_eq!(
        target.window.id, 1,
        "the window unused longest closes first"
    );
    assert_eq!(target.max_windows, None);
    assert_eq!(target.max_idle_minutes, Some(60));
}

#[test]
fn using_a_window_restarts_its_idle_clock_and_protects_the_focused_one() {
    let mut planner = Planner::default();
    let windows = snapshot(&[1, 2]);
    let settings = Settings {
        enabled: true,
        rules: vec![idle_rule("test.editor", 60)],
        ..Settings::default()
    };
    planner.observe(&windows, 0);
    // Window 1 was used half an hour in, so it is not idle yet at the hour.
    planner.mark_active(1, 30 * 60_000);
    let target = planner
        .candidate(&settings, &windows, &[1, 2], 60 * 60_000)
        .unwrap();
    assert_eq!(target.window.id, 2);
    // A window the scan reports as focused counts as used right now.
    let focused = snapshot_with_focus(&[1, 2], Some(2));
    planner.observe(&focused, 90 * 60_000);
    assert_eq!(planner.idle_ms(2, 90 * 60_000), 0);
    assert_eq!(planner.idle_ms(1, 90 * 60_000), 60 * 60_000);
    let target = planner
        .candidate(&settings, &focused, &[1, 2], 90 * 60_000)
        .unwrap();
    assert_eq!(target.window.id, 1, "the focused window never closes first");
    // The window that was just used is protected from the close itself too.
    let mut protected = focused.clone();
    protected[0].windows[0].protected = true;
    assert!(
        planner
            .candidate(&settings, &protected, &[2, 1], 91 * 60_000)
            .is_none()
    );
}

#[test]
fn a_rule_can_limit_windows_and_idle_time_together() {
    let mut planner = Planner::default();
    let windows = snapshot(&[1, 2, 3, 4]);
    planner.observe(&windows, 0);
    let settings = Settings {
        enabled: true,
        rules: vec![Rule {
            max_windows: Some(3),
            max_idle_minutes: Some(30),
            ..rule("test.editor", 3)
        }],
        ..Settings::default()
    };
    // Above the window limit, the least recently used window closes first.
    let target = planner
        .candidate(&settings, &windows, &[1, 2, 3, 4], 20_000)
        .unwrap();
    assert_eq!(target.reason, Reason::WindowLimit);
    assert_eq!(target.window.id, 4);
    // At the limit, only a window past its idle time is left to close, and the
    // one that has gone unused the longest goes first.
    let at_limit = snapshot(&[1, 2, 3]);
    planner.observe(&at_limit, 30 * 60_000);
    planner.mark_active(1, 30 * 60_000);
    planner.mark_active(2, 30 * 60_000);
    let target = planner
        .candidate(&settings, &at_limit, &[1, 2, 3], 61 * 60_000)
        .unwrap();
    assert_eq!(target.reason, Reason::Idle);
    assert_eq!(target.window.id, 3);
    assert_eq!(planner.idle_ms(3, 61 * 60_000), 61 * 60_000);
    assert_eq!(planner.idle_ms(1, 61 * 60_000), 31 * 60_000);
    // An idle rule still respects the grace period for new windows.
    let mut fresh = Planner::default();
    fresh.observe(&snapshot(&[9]), 31 * 60_000);
    assert!(
        fresh
            .candidate(&settings, &snapshot(&[9]), &[9], 31 * 60_000)
            .is_none()
    );
}

#[test]
fn idle_settings_survive_a_round_trip_and_a_rule_without_limits_is_rejected() {
    let config = Config {
        auto_appclose: Settings {
            enabled: true,
            rules: vec![idle_rule("test.editor", 240)],
            ..Settings::default()
        },
        ..Config::default()
    };
    let loaded = Config::from_json(&config.to_json().unwrap()).unwrap();
    assert_eq!(loaded, config);
    assert_eq!(loaded.auto_appclose.rules[0].max_windows, None);
    assert_eq!(loaded.auto_appclose.rules[0].max_idle_minutes, Some(240));
    // A rule written before idle limits existed still loads and still limits windows.
    let legacy = serde_json::json!({
        "auto_appclose": {
            "enabled": true,
            "interval_secs": 10,
            "rules": [{
                "application": {"bundle_id": "test.editor", "name": "Editor", "path": "/Applications/Editor.app"},
                "max_windows": 2
            }]
        }
    });
    let loaded = Config::from_json(&legacy.to_string()).unwrap();
    assert_eq!(loaded.auto_appclose.rules[0].max_windows, Some(2));
    assert_eq!(loaded.auto_appclose.rules[0].max_idle_minutes, None);
    // Limits outside the accepted range are rejected, not clamped.
    for bad in [0, MAX_IDLE_MINUTES + 1, u16::MAX] {
        let mut config = config.clone();
        config.auto_appclose.rules[0].max_idle_minutes = Some(bad);
        assert!(config.validate().is_err());
        assert!(Config::from_json(&serde_json::to_string(&config).unwrap()).is_err());
    }
}
