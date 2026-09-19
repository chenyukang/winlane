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
