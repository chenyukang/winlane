use super::*;

pub(super) fn verify_window_liveness(mtm: MainThreadMarker) {
    for mode in [PanelMode::Switch, PanelMode::Search] {
        let delegate = responsive_fixture(mtm);
        let state = delegate.ivars();
        state.mode.set(Some(mode));
        delegate.filter();
        if mode == PanelMode::Switch {
            state
                .switch_selection
                .replace(Some(SwitchSelection::new(1)));
            delegate.prepare_switch_selection();
        } else {
            state.selected.set(1);
        }
        assert_eq!(delegate.selected_window().unwrap().id, 2);
        // A later recency event must not reorder an already held switch gesture.
        state.recency.replace(vec![3, 2, 1]);
        state
            .window_server_ids
            .replace(HashMap::from([(1, 101), (2, 102), (3, 103)]));
        state
            .deferred_windows
            .replace(Some(state.windows.borrow().clone()));
        let (_scan_tx, scan_rx) = mpsc::channel();
        state.receiver.replace(Some(scan_rx));
        delegate.remove_closed_windows(&[999]);
        assert!(
            state.closed_windows.borrow().is_empty(),
            "destroyed controls are not window removals"
        );
        let old_snapshot = WindowSnapshot {
            windows: state.windows.borrow().clone(),
            identities: HashMap::new(),
            server_ids: state.window_server_ids.borrow().clone(),
        };
        let (tx, rx) = mpsc::channel();
        state.window_check_receiver.replace(Some(rx));
        tx.send(vec![1]).unwrap();
        delegate.poll_window_liveness();
        assert_eq!(delegate.selected_window().unwrap().id, 2);
        assert_eq!(
            state
                .windows
                .borrow()
                .iter()
                .map(|w| w.id)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert!(
            state
                .deferred_windows
                .borrow()
                .as_ref()
                .unwrap()
                .iter()
                .all(|w| w.id != 1)
        );
        assert!(
            state.closed_windows.borrow().contains(&1),
            "reject a late AX snapshot containing the closed window"
        );
        assert!(!state.window_server_ids.borrow().contains_key(&1));
        if mode == PanelMode::Switch {
            let windows = state.windows.borrow();
            assert_eq!(
                state
                    .matches
                    .borrow()
                    .iter()
                    .map(|&i| windows[i].id)
                    .collect::<Vec<_>>(),
                vec![2, 3]
            );
            drop(windows);
            delegate.move_selection(-1);
            assert_eq!(delegate.selected_window().unwrap().id, 3);
            delegate.move_selection(1);
        }
        state.receiver.take();
        delegate.install_window_snapshot(old_snapshot);
        assert!(state.windows.borrow().iter().all(|w| w.id != 1));
        assert!(!state.window_server_ids.borrow().contains_key(&1));
        assert!(
            state
                .deferred_windows
                .borrow()
                .as_ref()
                .unwrap()
                .iter()
                .all(|w| w.id != 1)
        );
        delegate.remove_closed_windows(&[2]);
        assert_eq!(delegate.selected_window().unwrap().id, 3);
        delegate.remove_closed_windows(&[3]);
        assert_eq!(delegate.match_count(), 0);
        assert!(delegate.selected_window().is_none());
        delegate.end_session();
        assert!(
            state.windows.borrow().is_empty(),
            "deferred rows must not resurrect closed windows"
        );
    }

    let delegate = responsive_fixture(mtm);
    let state = delegate.ivars();
    state.mode.set(Some(PanelMode::Search));
    delegate.filter();
    let before = state.windows.borrow().clone();
    for closed in [Some(Vec::new()), None] {
        let (tx, rx) = mpsc::channel();
        state.window_check_receiver.replace(Some(rx));
        if let Some(closed) = closed {
            tx.send(closed).unwrap();
        }
        drop(tx);
        delegate.poll_window_liveness();
        assert_eq!(
            *state.windows.borrow(),
            before,
            "failed inventory reads must preserve cached windows"
        );
        assert!(state.window_check_receiver.borrow().is_none());
    }
}

/// A row can outlive its window when the list was built before the close and
/// the background inventory check has not caught up. Selecting it must drop it
/// instead of asking for a manual refresh.
pub(super) fn verify_missing_window_row_is_dropped(mtm: MainThreadMarker) {
    let delegate = responsive_fixture(mtm);
    let state = delegate.ivars();
    state.mode.set(Some(PanelMode::Switch));
    delegate.filter();
    let window = state.windows.borrow()[0].clone();
    // Window numbers are small, so these can never exist in the WindowServer
    // inventory; a real number here would make the check depend on the machine.
    state.window_server_ids.replace(HashMap::from([
        (1, u32::MAX - 1),
        (2, u32::MAX - 2),
        (3, u32::MAX - 3),
    ]));
    // The fixture's negative process identifiers cannot be running or
    // installed, which is exactly the stale-row case.
    assert!(
        !crate::macos::platform::accessibility::window_is_open(window.pid, window.id),
        "a window of a nonexistent process must not report as open"
    );
    assert!(
        delegate.discard_missing_window(&window),
        "a window that is gone must be dropped"
    );
    assert!(
        state.windows.borrow().iter().all(|w| w.id != window.id),
        "the stale row must leave the list without a manual refresh"
    );
    assert!(
        !state.window_server_ids.borrow().contains_key(&window.id),
        "its server identity must be forgotten too"
    );
    assert_eq!(delegate.match_count(), 2);
    assert_eq!(delegate.selected_window().unwrap().id, 2);
}

/// A list that stays in front of the user is re-checked on a slow interval, so
/// a window that closes behind it disappears without a manual refresh.
pub(super) fn verify_open_panel_rechecks_window_liveness(mtm: MainThreadMarker) {
    let delegate = responsive_fixture(mtm);
    let state = delegate.ivars();
    state.demo.set(false);
    state
        .window_server_ids
        .replace(HashMap::from([(1, 101), (2, 102)]));
    state.window_check_receiver.take();
    state.window_check_at.take();
    state.mode.set(None);
    delegate.tick_window_liveness();
    assert!(
        state.window_check_receiver.borrow().is_none(),
        "a hidden list must not start inventory checks"
    );
    state.mode.set(Some(PanelMode::Search));
    delegate.tick_window_liveness();
    let receiver = state.window_check_receiver.borrow();
    assert!(
        receiver.is_some(),
        "an open list must re-check the windows it shows"
    );
    drop(receiver);
    state.window_check_receiver.take();
    let now = std::time::Instant::now();
    assert!(
        !crate::macos::app::window_liveness::liveness_due(Some(now), now),
        "checks are spaced out"
    );
    assert!(crate::macos::app::window_liveness::liveness_due(None, now));
    assert!(crate::macos::app::window_liveness::liveness_due(
        Some(now),
        now + crate::macos::app::window_liveness::LIVENESS_INTERVAL
    ));
    state.window_check_at.take();
}
