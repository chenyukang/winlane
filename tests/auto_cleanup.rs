use winlane::core::config::{ApplicationTarget, Config};
use winlane::features::auto_cleanup::{Planner, Rule, Settings, Snapshot, Window};

fn rule(bundle: &str, max_windows: u16) -> Rule {
    Rule {
        application: ApplicationTarget {
            bundle_id: bundle.into(),
            name: "Editor".into(),
            path: "/Applications/Editor.app".into(),
        },
        max_windows,
    }
}
fn settings() -> Settings {
    Settings {
        enabled: true,
        rules: vec![rule("test.editor", 3)],
    }
}
fn snapshot(ids: &[u64]) -> Vec<Snapshot> {
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
    }]
}

#[test]
fn defaults_are_off_and_rules_survive_disabling_and_restart() {
    assert_eq!(
        Config::from_json("{}").unwrap().auto_cleanup,
        Settings::default()
    );
    let mut config = Config {
        auto_cleanup: settings(),
        ..Config::default()
    };
    config.auto_cleanup.enabled = false;
    let loaded = Config::from_json(&config.to_json().unwrap()).unwrap();
    assert_eq!(loaded, config);
    assert_eq!(loaded.auto_cleanup.rules[0].max_windows, 3);
}

#[test]
fn validates_limits_duplicates_and_self_cleanup() {
    let mut s = settings();
    for limit in [0, 101, u16::MAX] {
        s.rules[0].max_windows = limit;
        assert!(s.validate().is_err());
    }
    for limit in [1, 3, 100] {
        s.rules[0].max_windows = limit;
        assert!(s.validate().is_ok());
    }
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
    assert!(p.candidate(&settings(), &s, &[1, 3, 4, 2], 9999).is_none());
    assert_eq!(
        p.candidate(&settings(), &s, &[1, 3, 4, 2], 10000)
            .unwrap()
            .window
            .id,
        2
    );
    let mut off = settings();
    off.enabled = false;
    assert!(p.candidate(&off, &s, &[1, 3, 4, 2], 10000).is_none());
    assert!(
        p.candidate(&settings(), &snapshot(&[1, 2, 3]), &[1, 2, 3], 10000)
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
    p.observe(&s, 10000);
    assert_eq!(
        p.candidate(&settings(), &s, &[1, 2, 3, 4], 10000)
            .unwrap()
            .window
            .id,
        2
    );
    assert_eq!(
        p.candidate(&settings(), &s, &[1, 2, 3, 4], 20000)
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
    let target = p.candidate(&settings(), &s, &[1, 2, 3, 4], 10000).unwrap();
    p.attempted(target.clone());
    assert!(
        p.candidate(&settings(), &s, &[1, 2, 3, 4], 999999)
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
    p.observe(&both, 10000);
    assert_eq!(
        p.candidate(&rules, &both, &[], 20000).unwrap().bundle_id,
        "test.other"
    );
    assert_eq!(p.confirm_closed(&[4]), vec![target]);
    assert!(
        p.candidate(&settings(), &snapshot(&[1, 2, 3]), &[], 20000)
            .is_none()
    );
}

#[test]
fn missing_or_failed_app_scan_never_closes_and_reappearance_gets_grace() {
    let mut p = Planner::default();
    p.observe(&snapshot(&[1, 2, 3, 4]), 0);
    p.observe(&[], 10000);
    assert!(p.candidate(&settings(), &[], &[], 20000).is_none());
    let s = snapshot(&[1, 2, 3, 4]);
    p.observe(&s, 20000);
    assert!(p.candidate(&settings(), &s, &[], 20000).is_none());
}

#[test]
fn same_bundle_across_processes_counts_as_one_rule_and_latest_recency_wins() {
    let mut p = Planner::default();
    let mut s = snapshot(&[1, 2, 3, 4]);
    s[0].windows[3].pid = 20;
    p.observe(&s, 0);
    assert_eq!(
        p.candidate(&settings(), &s, &[1, 2, 3, 4], 10000)
            .unwrap()
            .window
            .pid,
        20
    );
    assert_eq!(
        p.candidate(&settings(), &s, &[4, 1, 2, 3], 10000)
            .unwrap()
            .window
            .id,
        3
    );
}
