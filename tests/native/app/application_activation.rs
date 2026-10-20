use super::*;
use crate::macos::app::recency::application_visit;

type ApplicationWindowSender = mpsc::Sender<Option<(WindowInfo, Option<u32>)>>;

fn pending_application(delegate: &Delegate, pid: i32) -> ApplicationWindowSender {
    let (tx, rx) = mpsc::channel();
    delegate.request_focus(pid, Some(application_visit(pid)));
    delegate
        .ivars()
        .application_window_receivers
        .borrow_mut()
        .push(PendingApplicationWindow {
            pid,
            app: format!("App {pid}"),
            receiver: rx,
        });
    tx
}

fn window(id: u64, pid: i32) -> WindowInfo {
    WindowInfo {
        id,
        pid,
        app: format!("App {pid}"),
        title: format!("Window {id}"),
        minimized: false,
    }
}

fn begin_switch(delegate: &Delegate) {
    let state = delegate.ivars();
    state.mode.set(Some(PanelMode::Switch));
    state
        .switch_selection
        .replace(Some(SwitchSelection::new(1)));
    delegate.filter();
    delegate.prepare_switch_selection();
}

pub(super) fn verify_application_activation(mtm: MainThreadMarker) {
    // The provisional visit must survive replacement of a still-pending read.
    let delegate = responsive_fixture(mtm);
    delegate.request_focus(-3, Some(3));
    delegate.request_focus(-1, Some(1));
    assert_eq!(*delegate.ivars().recency.borrow(), [1, 3, 2]);

    // ZenRSSReader starts, opens Glide, then Command-Tab is pressed before its
    // focused window has been discovered. Keep the gesture pending, not stale.
    let delegate = responsive_fixture(mtm);
    let state = delegate.ivars();
    let rss = pending_application(&delegate, -4);
    delegate.remember_window(1); // Glide is already cached.
    let exact = pending_test_focus(&delegate, -1);
    begin_switch(&delegate);
    assert!(
        state
            .switch_selection
            .borrow()
            .as_ref()
            .unwrap()
            .selected()
            .is_none()
    );
    // Queue another Tab and release before discovery, without activating an app.
    {
        let mut selection = state.switch_selection.borrow_mut();
        let selection = selection.as_mut().unwrap();
        selection.step(1);
        selection.release();
    }
    rss.send(Some((window(4, -4), None))).unwrap();
    // Prevent poll_application_windows from committing: test queued navigation
    // separately from the application's real activation side effects.
    state.mode.set(None);
    delegate.poll_application_windows();
    assert_eq!(*state.recency.borrow(), [1, 4, 2, 3]);
    exact.send(Some(1)).unwrap();
    delegate.poll_focus();
    assert_eq!(
        *state.recency.borrow(),
        [1, 4, 2, 3],
        "an exact query's baseline must include the newly discovered visit"
    );
    state.mode.set(Some(PanelMode::Switch));
    delegate.filter();
    delegate.prepare_switch_selection();
    let index = state
        .switch_selection
        .borrow_mut()
        .as_mut()
        .unwrap()
        .take_commit()
        .unwrap();
    assert_eq!(
        state.windows.borrow()[state.matches.borrow()[index]].id,
        2,
        "two queued Tabs must include the new app in navigation"
    );
    begin_switch(&delegate);
    assert_eq!(
        delegate.selected_window().unwrap().id,
        4,
        "a single Command-Tab must return to the app that opened the link"
    );
    let frozen = state.matches.borrow().clone();
    let later = pending_application(&delegate, -5);
    later.send(Some((window(5, -5), None))).unwrap();
    delegate.poll_application_windows();
    assert_eq!(
        *state.matches.borrow(),
        frozen,
        "discovery must not reshuffle an already held gesture"
    );
    assert!(!state.windows.borrow().iter().any(|window| window.id == 5));
    assert!(
        state
            .deferred_windows
            .borrow()
            .as_ref()
            .unwrap()
            .iter()
            .any(|window| window.id == 5)
    );
    delegate.end_session();
    assert_eq!(*state.recency.borrow(), [5, 1, 4, 2, 3]);

    // Discovery completion order is not visit order. An older full scan must
    // not erase windows delivered independently after the scan began.
    let delegate = responsive_fixture(mtm);
    let state = delegate.ivars();
    let old_windows = state.windows.borrow().clone();
    let (_snapshot_tx, rx) = mpsc::channel();
    state.receiver.replace(Some(rx));
    let first = pending_application(&delegate, -4);
    let second = pending_application(&delegate, -5);
    delegate.remember_window(1);
    second.send(Some((window(5, -5), None))).unwrap();
    delegate.poll_application_windows();
    first.send(Some((window(4, -4), None))).unwrap();
    delegate.poll_application_windows();
    assert_eq!(*state.recency.borrow(), [1, 5, 4, 2, 3]);
    state.receiver.take();
    delegate.install_window_snapshot(WindowSnapshot {
        windows: old_windows,
        identities: HashMap::new(),
        server_ids: HashMap::new(),
    });
    assert!(state.windows.borrow().iter().any(|window| window.id == 4));
    assert!(state.windows.borrow().iter().any(|window| window.id == 5));
    assert_eq!(*state.recency.borrow(), [1, 5, 4, 2, 3]);

    // Escape queued before discovery completes must cancel the released
    // gesture before a newly available window can trigger its commit.
    let delegate = responsive_fixture(mtm);
    let state = delegate.ivars();
    let rss = pending_application(&delegate, -4);
    delegate.remember_window(1);
    begin_switch(&delegate);
    state
        .switch_selection
        .borrow_mut()
        .as_mut()
        .unwrap()
        .release();
    let (tx, rx) = mpsc::channel();
    state.shortcut_rx.replace(Some(rx));
    tx.send(Action {
        session: state.session.get(),
        kind: ActionKind::Cancel,
    })
    .unwrap();
    rss.send(Some((window(4, -4), None))).unwrap();
    state.last_shortcut_check.set(Some(Instant::now()));
    delegate.poll(sel!(poll:), None);
    assert_eq!(
        state.mode.get(),
        None,
        "a queued cancellation wins over discovery completion"
    );

    // Both results may already be queued in one run-loop turn.
    let delegate = responsive_fixture(mtm);
    let first = pending_application(&delegate, -4);
    let second = pending_application(&delegate, -5);
    delegate.remember_window(1);
    first.send(Some((window(4, -4), None))).unwrap();
    second.send(Some((window(5, -5), None))).unwrap();
    delegate.poll_application_windows();
    assert_eq!(*delegate.ivars().recency.borrow(), [1, 5, 4, 2, 3]);

    // A scan can finish before the new app's targeted read, too. Its newer
    // bundle identity must remain available for aliases and window icons.
    let delegate = responsive_fixture(mtm);
    let state = delegate.ivars();
    let rss = pending_application(&delegate, -4);
    state.identities.borrow_mut().insert(
        -4,
        AppIdentity {
            id: "example.rss".into(),
            english_name: "RSS".into(),
        },
    );
    let windows = state.windows.borrow().clone();
    delegate.install_window_snapshot(WindowSnapshot {
        windows,
        identities: HashMap::new(),
        server_ids: HashMap::new(),
    });
    assert_eq!(state.identities.borrow()[&-4].id, "example.rss");
    rss.send(Some((window(4, -4), Some(404)))).unwrap();
    delegate.poll_application_windows();
    assert_eq!(state.window_server_ids.borrow()[&4], 404);
    assert_eq!(*state.recency.borrow(), [4, 1, 2, 3]);

    // Full discovery can resolve an activation slot before its targeted read;
    // neither completion should create a duplicate or a second, newer visit.
    let delegate = responsive_fixture(mtm);
    let state = delegate.ivars();
    let rss = pending_application(&delegate, -4);
    delegate.remember_window(1);
    let mut windows = state.windows.borrow().clone();
    windows.push(window(4, -4));
    delegate.install_windows(windows);
    assert_eq!(*state.recency.borrow(), [1, 4, 2, 3]);
    rss.send(Some((window(4, -4), None))).unwrap();
    delegate.poll_application_windows();
    assert_eq!(*state.recency.borrow(), [1, 4, 2, 3]);
    assert_eq!(
        state
            .windows
            .borrow()
            .iter()
            .filter(|window| window.id == 4)
            .count(),
        1
    );

    // A newly activated browser can still be reading its own window while the
    // source app is already cached. Switching back must not wait for the browser.
    let delegate = responsive_fixture(mtm);
    let state = delegate.ivars();
    delegate.remember_window(2); // RSS reader.
    let _browser = pending_application(&delegate, -5);
    state.previous_pid.set(-5);
    state.previous_window.set(None);
    state.loading.set(true); // A full scan is also still pending.
    begin_switch(&delegate);
    assert!(!delegate.waiting_for_application_window());
    assert_eq!(delegate.selected_window().unwrap().id, 2);
    assert_eq!(
        state.application_window_receivers.borrow().len(),
        1,
        "the cached target must be usable before the unrelated read finishes"
    );
    delegate.end_session();

    // An older, unrelated app must not make a normal one-step switch wait on
    // the entire inventory. Two queued steps really do target that app, though.
    let delegate = responsive_fixture(mtm);
    let state = delegate.ivars();
    let older = pending_application(&delegate, -4);
    delegate.remember_window(2);
    delegate.remember_window(1);
    state.loading.set(true);
    begin_switch(&delegate);
    assert_eq!(delegate.selected_window().unwrap().id, 2);
    state
        .switch_selection
        .replace(Some(SwitchSelection::new(1)));
    state
        .switch_selection
        .borrow_mut()
        .as_mut()
        .unwrap()
        .step(1);
    delegate.filter();
    delegate.prepare_switch_selection();
    assert!(delegate.waiting_for_application_window());
    assert!(
        state
            .switch_selection
            .borrow()
            .as_ref()
            .unwrap()
            .selected()
            .is_none()
    );
    older.send(Some((window(4, -4), None))).unwrap();
    delegate.poll_application_windows();
    assert_eq!(delegate.selected_window().unwrap().id, 4);
    delegate.end_session();

    // Reverse navigation and explicitly excluded apps obey the same target
    // projection, rather than waiting for every outstanding application read.
    let delegate = responsive_fixture(mtm);
    let state = delegate.ivars();
    let _rss = pending_application(&delegate, -4);
    delegate.remember_window(1);
    state.mode.set(Some(PanelMode::Switch));
    state
        .switch_selection
        .replace(Some(SwitchSelection::new(-1)));
    delegate.filter();
    delegate.prepare_switch_selection();
    assert_eq!(delegate.selected_window().unwrap().id, 3);
    state
        .config
        .borrow_mut()
        .excluded_apps
        .push("App -4".into());
    begin_switch(&delegate);
    assert_eq!(delegate.selected_window().unwrap().id, 2);
    delegate.end_session();

    // Apps with no readable standard window must not leave a gesture stuck.
    for disconnected in [false, true] {
        let delegate = responsive_fixture(mtm);
        let tx = pending_application(&delegate, -4);
        delegate.remember_window(1);
        begin_switch(&delegate);
        assert!(delegate.waiting_for_application_window());
        if !disconnected {
            tx.send(None).unwrap();
        }
        drop(tx);
        delegate.poll_application_windows();
        assert!(!delegate.waiting_for_application_window());
        assert_eq!(delegate.selected_window().unwrap().id, 2);
        delegate.end_session();
    }
    println!(
        "Application activation checks passed: new-app handoff, immediate switch-back, queued navigation, out-of-order discovery, exact-query correction, stale full scans, frozen gestures, target-specific waiting, reverse/excluded navigation and failed reads; no apps activated."
    );
}
