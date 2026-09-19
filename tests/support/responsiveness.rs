fn responsive_fixture(mtm: MainThreadMarker) -> Retained<Delegate> {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.demo.set(true);
    state.windows.replace(
        (1..=3)
            .map(|id| WindowInfo {
                id,
                pid: -(id as i32),
                app: format!("App {id}"),
                title: format!("Window {id}"),
                minimized: false,
            })
            .collect(),
    );
    state.recency.replace(vec![1, 2, 3]);
    state.previous_pid.set(-1);
    state.previous_window.set(Some(1));
    delegate
}

fn pending_test_focus(delegate: &Delegate, pid: i32) -> mpsc::Sender<Option<u64>> {
    let state = delegate.ivars();
    let (tx, rx) = mpsc::channel();
    state.focus_pid.set(pid);
    state.focus_receiver.replace(Some(PendingFocus {
        receiver: rx,
        pid,
        revision: state.focus_revision.get(),
        recency: state.recency.borrow().clone(),
    }));
    tx
}

fn verify_responsive_panels(mtm: MainThreadMarker) {
    let delegate = responsive_fixture(mtm);
    let state = delegate.ivars();
    // Both workers remain pending while the cached panel becomes usable.
    let (_windows_tx, windows_rx) = mpsc::channel();
    state.receiver.replace(Some(windows_rx));
    let _focus_tx = pending_test_focus(&delegate, -1);
    state.config.borrow_mut().switch_delay_ms = 750;
    for mode in [PanelMode::Search, PanelMode::Switch] {
        let before = state.render_passes.get();
        delegate.prepare_panel(mode, 10, 1);
        assert_eq!(state.render_passes.get() - before, delegate.panels().len());
        assert_eq!(delegate.match_count(), 3);
        assert_eq!(
            delegate.selected_window().unwrap().id,
            if mode == PanelMode::Switch { 2 } else { 1 }
        );
        assert!(state.receiver.borrow().is_some());
        assert!(state.focus_receiver.borrow().is_some());
        assert!(
            state.icons.borrow().is_empty(),
            "presenting must not fetch application icons"
        );
        assert!(!delegate.any_panel_visible());
        assert_eq!(state.config.borrow().switch_delay_ms, 750);
    }
    // Prewarming is bounded per turn and uses the existing row pool.
    let selected = delegate.selected_window().unwrap().id;
    assert!(delegate.warm_cache_step());
    assert_eq!(state.icons.borrow().len(), 1);
    assert_eq!(delegate.selected_window().unwrap().id, selected);
    let mut steps = 0;
    while delegate.warm_cache_step() {
        steps += 1;
        assert!(steps < 20);
    }
    assert_eq!(state.icons.borrow().len(), 3);
    state.demo.set(false);
    delegate.schedule_cache_warmup();
    let timer = state.cache_warmup_timer.borrow().as_ref().unwrap().clone();
    timer.fire();
    assert!(!timer.isValid());
    assert!(
        state.cache_warmup_timer.borrow().is_none(),
        "finished warmup must stop waking the idle app"
    );
    state.demo.set(true);
    delegate.end_session();
    for ui in delegate.panels() {
        ui.panel.close();
    }

    let delegate = responsive_fixture(mtm);
    delegate.sync_displays();
    let state = delegate.ivars();
    for window in state.windows.borrow().iter() {
        state.icons.borrow_mut().insert(window.pid, None);
    }
    assert!(delegate.warm_cache_step());
    assert_eq!(
        delegate
            .panels()
            .iter()
            .map(|ui| ui.rows.borrow().len())
            .sum::<usize>(),
        1
    );
    assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));
    for ui in delegate.panels() {
        ui.panel.close();
    }
}

fn verify_async_focus_order(mtm: MainThreadMarker) {
    for disconnected in [false, true] {
        let delegate = responsive_fixture(mtm);
        let state = delegate.ivars();
        state.recency.replace(vec![2, 3, 1]);
        let tx = pending_test_focus(&delegate, -1);
        if !disconnected {
            tx.send(None).unwrap();
        }
        drop(tx);
        delegate.poll_focus();
        assert_eq!(
            *state.recency.borrow(),
            [1, 2, 3],
            "a failed focus read must still record the activated app's cached window"
        );
        delegate.remember_window(2);
        delegate.filter();
        assert_eq!(
            state
                .matches
                .borrow()
                .iter()
                .map(|&index| state.windows.borrow()[index].id)
                .collect::<Vec<_>>(),
            [2, 1, 3],
            "the app just left must follow the app switched to"
        );

        let tx = pending_test_focus(&delegate, -1);
        delegate.remember_window(3);
        tx.send(None).unwrap();
        delegate.poll_focus();
        assert_eq!(
            *state.recency.borrow(),
            [3, 2, 1],
            "a failed stale query must not overwrite a newer visit"
        );
        let tx = pending_test_focus(&delegate, -1);
        state.focus_pid.set(-2);
        tx.send(None).unwrap();
        delegate.poll_focus();
        assert_eq!(*state.recency.borrow(), [3, 2, 1]);
        let tx = pending_test_focus(&delegate, -99);
        tx.send(None).unwrap();
        delegate.poll_focus();
        assert_eq!(*state.recency.borrow(), [3, 2, 1]);
    }

    let delegate = responsive_fixture(mtm);
    let state = delegate.ivars();
    state.mode.set(Some(PanelMode::Switch));
    state
        .switch_selection
        .replace(Some(SwitchSelection::new(1)));
    delegate.filter();
    delegate.prepare_switch_selection();
    let selection = delegate.selected_window().unwrap().id;
    let order = state.matches.borrow().clone();
    state.windows.borrow_mut()[2].pid = -1;
    state.recency.replace(vec![2, 1, 3]);
    let tx = pending_test_focus(&delegate, -1);
    // A provisional sibling is not an actual visit. Restore its old position
    // when the background query identifies the real focused window.
    let before = state.recency.borrow().clone();
    delegate.remember_window(1);
    state.focus_receiver.borrow_mut().as_mut().unwrap().revision = state.focus_revision.get();
    delegate.poll_focus();
    assert_eq!(delegate.selected_window().unwrap().id, selection);
    tx.send(Some(3)).unwrap();
    delegate.poll_focus();
    assert_eq!(state.recency.borrow()[0], 3);
    assert_eq!(
        *state.recency.borrow(),
        std::iter::once(3)
            .chain(before.into_iter().filter(|id| *id != 3))
            .collect::<Vec<_>>()
    );
    assert_eq!(state.previous_window.get(), Some(3));
    assert_eq!(delegate.selected_window().unwrap().id, selection);
    assert_eq!(
        *state.matches.borrow(),
        order,
        "a held gesture must not reorder"
    );

    let tx = pending_test_focus(&delegate, -1);
    delegate.remember_window(2); // A more recent explicit activation wins.
    tx.send(Some(1)).unwrap();
    delegate.poll_focus();
    assert_eq!(state.recency.borrow()[0], 2);

    let tx = pending_test_focus(&delegate, -1);
    state.focus_pid.set(-2); // An external app switch invalidates the old query.
    tx.send(Some(1)).unwrap();
    delegate.poll_focus();
    assert_eq!(state.recency.borrow()[0], 2);

    state.mode.set(Some(PanelMode::Search));
    delegate.filter();
    let selected = delegate.selected_window().unwrap().id;
    let tx = pending_test_focus(&delegate, -1);
    tx.send(Some(1)).unwrap();
    delegate.poll_focus();
    assert_eq!(state.recency.borrow()[0], 1);
    assert_eq!(state.windows.borrow()[state.matches.borrow()[0]].id, 1);
    assert_eq!(delegate.selected_window().unwrap().id, selected);
    let tx = pending_test_focus(&delegate, -1);
    tx.send(None).unwrap();
    delegate.poll_focus();
    assert_eq!(delegate.selected_window().unwrap().id, selected);
    delegate.end_session();
}

fn verify_async_window_snapshot(mtm: MainThreadMarker) {
    let delegate = responsive_fixture(mtm);
    let state = delegate.ivars();
    state.demo.set(false);
    // Exercise result delivery without registering global shortcuts or scanning.
    state.last_shortcut_check.set(Some(Instant::now()));
    state.mode.set(Some(PanelMode::Search));
    delegate.filter();
    state.selected.set(1);
    for mode in [PanelMode::Search, PanelMode::Switch] {
        state.mode.set(Some(mode));
        if mode == PanelMode::Switch {
            state
                .switch_selection
                .replace(Some(SwitchSelection::new(1)));
            delegate.prepare_switch_selection();
        }
        let before = state.windows.borrow().clone();
        let selected = delegate.selected_window().unwrap().id;
        let mut refreshed = before.clone();
        refreshed.reverse();
        refreshed[0].title = "Updated title".into();
        let (tx, rx) = mpsc::channel();
        state.receiver.replace(Some(rx));
        tx.send(WindowSnapshot {
            windows: refreshed.clone(),
            identities: HashMap::new(),
        })
        .unwrap();
        delegate.poll(sel!(poll:), None);
        if accessibility::is_trusted() {
            assert_eq!(delegate.selected_window().unwrap().id, selected);
            if mode == PanelMode::Switch {
                assert_eq!(*state.windows.borrow(), before);
                assert_eq!(*state.deferred_windows.borrow(), Some(refreshed));
            } else {
                assert_eq!(*state.windows.borrow(), refreshed);
            }
        } else {
            assert!(state.windows.borrow().is_empty());
            break;
        }
    }
    delegate.end_session();
    // A reused PID or exited application must not retain an old icon.
    state.identities.replace(HashMap::from([(
        -1,
        AppIdentity {
            id: "example.old".into(),
            english_name: "Old".into(),
        },
    )]));
    state.icons.replace(HashMap::from([(-1, None), (-2, None)]));
    delegate.install_identities(HashMap::from([(
        -1,
        AppIdentity {
            id: "example.new".into(),
            english_name: "New".into(),
        },
    )]));
    assert!(state.icons.borrow().is_empty());
}
