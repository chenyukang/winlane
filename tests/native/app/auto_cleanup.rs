use super::*;
use winlane::features::auto_cleanup::{Rule, Snapshot, Window};

pub fn verify(mtm: MainThreadMarker) {
    let rule = Rule {
        application: ApplicationTarget {
            bundle_id: "test.cleanup.editor".into(),
            path: "/Applications/Editor.app".into(),
            name: "Editor".into(),
        },
        max_windows: 3,
    };
    let target = Target {
        bundle_id: rule.application.bundle_id.clone(),
        window: Window {
            id: 987654321,
            pid: i32::MAX,
            server_id: Some(u32::MAX),
            protected: false,
        },
        max_windows: 3,
    };
    let settings = Settings {
        enabled: true,
        rules: vec![rule.clone()],
        ..Settings::default()
    };
    verify_interval(mtm, &settings, &target);
    let delegate = Delegate::new(mtm);
    let app = delegate.ivars();
    app.config.borrow_mut().auto_cleanup = settings.clone();
    app.auto_cleanup.borrow_mut().configure(&settings);
    app.auto_cleanup.borrow_mut().next_scan = Some(Instant::now() + Duration::from_secs(3600));

    let (tx, rx) = mpsc::channel();
    let token = Arc::new(AtomicBool::new(false));
    let generation = app.auto_cleanup.borrow().generation;
    app.auto_cleanup.borrow_mut().job =
        Some(Job::Close(rx, target.clone(), generation, token.clone()));
    delegate.remember_window(42);
    assert!(
        token.load(Ordering::Acquire),
        "MRU changes cancel an obsolete close plan"
    );
    assert_eq!(
        cleanup::close(&target, &rule, &token),
        cleanup::CloseResult::Skipped,
        "cancelled work never calls AX"
    );
    delegate.poll_auto_cleanup();
    assert!(
        matches!(app.auto_cleanup.borrow().job, Some(Job::Close(..))),
        "only one job may run"
    );
    tx.send(cleanup::CloseResult::Requested).unwrap();
    delegate.poll_auto_cleanup();
    assert_eq!(
        app.auto_cleanup.borrow().planner.pending().count(),
        1,
        "a completed request remains pending even if cancellation arrived later"
    );

    let (scan_tx, scan_rx) = mpsc::channel();
    app.auto_cleanup.borrow_mut().job = Some(Job::Scan(scan_rx, generation));
    scan_tx
        .send(cleanup::Scan {
            apps: vec![],
            closed: vec![target.window.id],
        })
        .unwrap();
    delegate.poll_auto_cleanup();
    assert_eq!(app.auto_cleanup.borrow().planner.pending().count(), 0);

    let (_tx, rx) = mpsc::channel();
    let token = Arc::new(AtomicBool::new(false));
    app.auto_cleanup.borrow_mut().job =
        Some(Job::Close(rx, target.clone(), generation, token.clone()));
    let mut off = settings.clone();
    off.enabled = false;
    app.config.borrow_mut().auto_cleanup = off.clone();
    app.auto_cleanup.borrow_mut().configure(&off);
    assert!(
        token.load(Ordering::Acquire),
        "global off cancels pending actions immediately"
    );
    assert_eq!(app.auto_cleanup.borrow().settings.rules, settings.rules);
    app.auto_cleanup.borrow_mut().job.take();

    let (scan_tx, scan_rx) = mpsc::channel();
    app.auto_cleanup.borrow_mut().job = Some(Job::Scan(scan_rx, generation));
    scan_tx
        .send(cleanup::Scan {
            apps: vec![Snapshot {
                bundle_id: target.bundle_id.clone(),
                windows: vec![target.window],
            }],
            closed: vec![],
        })
        .unwrap();
    delegate.poll_auto_cleanup();
    assert!(
        app.auto_cleanup.borrow().job.is_none(),
        "late scan cannot restart disabled cleanup"
    );
    assert!(!app.auto_cleanup.borrow().settings.enabled);
    println!(
        "Auto Cleanup: MRU cancellation, global off, late workers, one in-flight operation and close confirmation verified; no real windows closed."
    );
}

fn verify_interval(mtm: MainThreadMarker, settings: &Settings, target: &Target) {
    let delegate = Delegate::new(mtm);
    let app = delegate.ivars();
    app.config.borrow_mut().auto_cleanup = settings.clone();
    let before = Instant::now();
    delegate.poll_auto_cleanup();
    let after = Instant::now();
    let next = app.auto_cleanup.borrow().next_scan.unwrap();
    assert!(next >= before + Duration::from_secs(10) && next <= after + Duration::from_secs(10));
    assert!(matches!(app.auto_cleanup.borrow().job, Some(Job::Scan(..))));
    // This scan only names a nonexistent test bundle. Never close a real window.
    app.auto_cleanup.borrow_mut().job.take();
    app.auto_cleanup
        .borrow_mut()
        .planner
        .attempted(target.clone());
    let generation = app.auto_cleanup.borrow().generation;
    let (_tx, rx) = mpsc::channel();
    let token = Arc::new(AtomicBool::new(false));
    app.auto_cleanup.borrow_mut().job =
        Some(Job::Close(rx, target.clone(), generation, token.clone()));
    let mut slower = settings.clone();
    slower.interval_secs = 30;
    app.config.borrow_mut().auto_cleanup = slower.clone();
    let before = Instant::now();
    app.auto_cleanup.borrow_mut().configure(&slower);
    let after = Instant::now();
    let state = app.auto_cleanup.borrow();
    let next = state.next_scan.unwrap();
    assert!(next >= before + Duration::from_secs(30) && next <= after + Duration::from_secs(30));
    assert!(token.load(Ordering::Acquire));
    assert_eq!(
        state.generation, generation,
        "interval edits still accept in-flight close confirmations"
    );
    assert_eq!(
        state.planner.pending().count(),
        1,
        "interval edits preserve pending save decisions"
    );
    drop(state);
    app.auto_cleanup.borrow_mut().job.take();
    delegate.poll_auto_cleanup();
    assert!(
        app.auto_cleanup.borrow().job.is_none(),
        "no scan before the new interval expires"
    );
}
