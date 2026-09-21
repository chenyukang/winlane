use super::*;

pub(super) fn verify_project_rule_search(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.catalog_checked.set(Some(Instant::now()));
    state.mode.set(Some(PanelMode::Search));
    state.identities.borrow_mut().insert(
        -10,
        AppIdentity {
            id: "com.example.code".into(),
            english_name: "Code".into(),
        },
    );
    state.config.borrow_mut().alias_rules = vec![winlane::core::config::AliasRule {
        alias: "ck".into(),
        title_contains: "ckb".into(),
        application: ApplicationTarget {
            bundle_id: "com.example.code".into(),
            name: "Code".into(),
            path: "/Applications/Code.app".into(),
        },
    }];
    state.installed_apps.replace(vec![InstalledApp {
        names: vec!["Ck Other".into()],
        target: ApplicationTarget {
            bundle_id: "com.example.other".into(),
            name: "Ck Other".into(),
            path: "/Applications/Ck Other.app".into(),
        },
    }]);
    let windows = vec![
        WindowInfo {
            id: 1,
            pid: -10,
            app: "Code".into(),
            title: "main.rs — ckb".into(),
            minimized: false,
        },
        WindowInfo {
            id: 2,
            pid: -10,
            app: "Code".into(),
            title: "lib.rs — rust".into(),
            minimized: false,
        },
    ];
    delegate.install_windows(windows.clone());
    state.query.replace("ck".into());
    delegate.filter();
    assert_eq!(delegate.match_count(), 3);
    assert_eq!(state.launch_matches.borrow().len(), 1);
    assert_eq!(delegate.selected_window().unwrap().id, 1);
    state.mode.set(Some(PanelMode::Switch));
    state.query.replace(String::new());
    state
        .switch_selection
        .replace(Some(SwitchSelection::new(0)));
    delegate.filter();
    delegate.prepare_switch_selection();
    for ch in ['c', 'k'] {
        delegate.shortcut_action(Action {
            session: 0,
            kind: ActionKind::Alias(ch),
        });
    }
    assert_eq!(delegate.selected_window().unwrap().id, 1);
    delegate.install_windows(vec![windows[1].clone()]);
    state.mode.set(Some(PanelMode::Search));
    state.query.replace("ck".into());
    delegate.filter();
    assert_eq!(
        delegate.match_count(),
        1,
        "closed project aliases must still allow ordinary name matches"
    );
    assert_eq!(
        delegate.selected_application().unwrap().bundle_id,
        "com.example.other"
    );
    let mut reopened = windows;
    reopened[0].id = 10;
    reopened[0].title = "README.md — CKB".into();
    delegate.install_windows(reopened);
    delegate.filter();
    assert_eq!(delegate.selected_window().unwrap().id, 10);
    state.config.borrow_mut().alias_rules.clear();
    delegate.update_aliases(&state.windows.borrow());
    assert_eq!(*state.aliases.borrow(), *state.automatic_aliases.borrow());
}

fn scheduled_switch(delegate: &Delegate, delay: f64) -> Retained<NSTimer> {
    let before = objc2_foundation::NSDate::timeIntervalSinceReferenceDate_class();
    delegate.schedule_switch_panel();
    let after = objc2_foundation::NSDate::timeIntervalSinceReferenceDate_class();
    let timer = delegate
        .ivars()
        .switch_timer
        .borrow()
        .as_ref()
        .unwrap()
        .clone();
    // A non-repeating NSTimer reports a zero interval; verify its deadline.
    assert!(
        (before + delay - 0.001..=after + delay + 0.001)
            .contains(&timer.fireDate().timeIntervalSinceReferenceDate())
    );
    timer
}

pub(super) fn verify_switch_delay(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.mode.set(Some(PanelMode::Switch));
    state
        .switch_selection
        .replace(Some(SwitchSelection::new(1)));
    let first = scheduled_switch(&delegate, 0.1);
    assert!(first.isValid());
    assert!(!delegate.any_panel_visible());
    state
        .switch_selection
        .borrow_mut()
        .as_mut()
        .unwrap()
        .release();
    first.fire();
    assert!(!delegate.any_panel_visible());
    assert!(
        state.switch_timer.borrow().is_some(),
        "released gestures must wait for selection without presenting"
    );
    delegate.end_session();
    assert!(!first.isValid());
    assert!(state.switch_timer.borrow().is_none());

    state.mode.set(Some(PanelMode::Switch));
    state.config.borrow_mut().switch_delay_ms = 750;
    state
        .switch_selection
        .replace(Some(SwitchSelection::new(1)));
    let held = scheduled_switch(&delegate, 0.75);
    unsafe {
        let _: () = msg_send![&*delegate, presentSwitch: &*first];
    }
    assert!(
        state.switch_timer.borrow().is_some(),
        "old callbacks must not reveal a newer gesture"
    );
    held.fire();
    assert!(
        state.switch_timer.borrow().is_none(),
        "holding reveals through the normal presentation path"
    );
    delegate.schedule_switch_panel();
    let search = state.switch_timer.borrow().as_ref().unwrap().clone();
    delegate.display_search(7);
    assert!(!search.isValid());
    assert!(state.switch_timer.borrow().is_none());
    assert_eq!(state.mode.get(), Some(PanelMode::Search));
    state.config.borrow_mut().switch_delay_ms = 0;
    delegate.schedule_switch_panel();
    assert!(state.switch_timer.borrow().is_none());
    assert!(
        !delegate.any_panel_visible(),
        "tests must not create visible windows"
    );
}

pub(super) fn verify_external_focus_history(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.windows.replace(
        [(1, -10), (2, -20), (3, -20), (4, -30)]
            .into_iter()
            .map(|(id, pid)| WindowInfo {
                id,
                pid,
                app: format!("App {pid}"),
                title: format!("Window {id}"),
                minimized: false,
            })
            .collect(),
    );
    // App activation followed by focus notifications, including a project
    // switch inside one app. A duplicate notification must not change history.
    for id in [1, 2, 4, 3, 3] {
        delegate.remember_window(id);
    }
    state.mode.set(Some(PanelMode::Switch));
    state.previous_pid.set(-20);
    state.previous_window.set(Some(3));
    state
        .switch_selection
        .replace(Some(SwitchSelection::new(1)));
    delegate.filter();
    delegate.prepare_switch_selection();
    assert_eq!(*state.recency.borrow(), [3, 4, 2, 1]);
    assert_eq!(delegate.selected_window().unwrap().id, 4);
    let snapshot = state.matches.borrow().clone();
    delegate.remember_window(1);
    assert_eq!(
        *state.matches.borrow(),
        snapshot,
        "focus events must not reshuffle an active gesture"
    );
    assert_eq!(delegate.selected_window().unwrap().id, 4);
    state
        .switch_selection
        .replace(Some(SwitchSelection::new(1)));
    state.previous_pid.set(-10);
    state.previous_window.set(Some(1));
    delegate.filter();
    delegate.prepare_switch_selection();
    assert_eq!(delegate.selected_window().unwrap().id, 3);
    assert!(crate::macos::platform::focus_observer::FocusObserver::new(-1, mtm, |_| {}).is_none());
}

pub(super) fn verify_shortcut_recency(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    // The hidden test process stands in for the launch completion's target;
    // no user application is activated and no keyboard shortcut is registered.
    let target_pid = std::process::id() as i32;
    state.windows.replace(
        [
            (10, target_pid),
            (11, target_pid),
            (20, -20),
            (21, -20),
            (30, -30),
        ]
        .into_iter()
        .map(|(id, pid)| WindowInfo {
            id,
            pid,
            app: format!("Application {pid}"),
            title: format!("Window {id}"),
            minimized: false,
        })
        .collect(),
    );
    for origin in [LaunchOrigin::Shortcut, LaunchOrigin::Search] {
        state.recency.replace(vec![20, 11, 30, 10, 21]);
        // Capture the exact departing project window, even when a sibling
        // window of the same application is already in the history.
        delegate.remember_window(21);
        for _ in 0..2 {
            let (tx, rx) = mpsc::channel();
            state.launch_receiver.replace(Some(PendingLaunch {
                receiver: rx,
                origin,
            }));
            let before = state.recency.borrow().clone();
            delegate.poll_app_launch();
            assert_eq!(
                *state.recency.borrow(),
                before,
                "pending launches are not visits"
            );
            tx.send(Ok(target_pid)).unwrap();
            delegate.poll_app_launch();
            state.previous_pid.set(target_pid);
            state.previous_window.set(Some(11));
            state.mode.set(Some(PanelMode::Switch));
            state
                .switch_selection
                .replace(Some(SwitchSelection::new(1)));
            delegate.filter();
            delegate.prepare_switch_selection();
            let ordered: Vec<_> = state
                .matches
                .borrow()
                .iter()
                .map(|&index| state.windows.borrow()[index].id)
                .collect();
            assert_eq!(
                ordered,
                [11, 21, 20, 30, 10],
                "app shortcuts must update MRU without grouping sibling windows"
            );
            assert_eq!(state.selected.get(), 1);
            assert_eq!(delegate.selected_window().unwrap().id, 21);
        }
        // Simulate accepting the second entry, then opening the switcher again.
        delegate.remember_window(21);
        state.previous_pid.set(-20);
        state.previous_window.set(Some(21));
        state
            .switch_selection
            .replace(Some(SwitchSelection::new(1)));
        delegate.filter();
        delegate.prepare_switch_selection();
        assert_eq!(state.recency.borrow()[..2], [21, 11]);
        assert_eq!(delegate.selected_window().unwrap().id, 11);

        state
            .switch_selection
            .replace(Some(SwitchSelection::new(-1)));
        delegate.prepare_switch_selection();
        assert_eq!(
            delegate.selected_window().unwrap().id,
            10,
            "reverse cycling must still wrap"
        );
    }
    let before = state.recency.borrow().clone();
    assert_eq!(delegate.remember_application(-999), None);
    assert_eq!(
        *state.recency.borrow(),
        before,
        "missing apps must not change history"
    );
    println!(
        "Recency checks passed: shortcut/search launches, departing project window, repeat shortcut, toggle back, reverse cycle, pending and missing targets."
    );
}

pub(super) fn verify_catalog_refresh(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.mode.set(Some(PanelMode::Search));
    for query in ["", "   "] {
        state.query.replace(query.into());
        delegate.ensure_app_catalog();
        assert!(
            state.catalog_receiver.borrow().is_none(),
            "empty search must not scan apps"
        );
    }
    state.query.replace("browser".into());
    state.current_app_only.set(true);
    delegate.ensure_app_catalog();
    assert!(
        state.catalog_receiver.borrow().is_none(),
        "window-only scope must not scan apps"
    );
    state.current_app_only.set(false);
    state.mode.set(Some(PanelMode::Switch));
    delegate.ensure_app_catalog();
    assert!(
        state.catalog_receiver.borrow().is_none(),
        "switch mode must not scan apps"
    );
    state.mode.set(Some(PanelMode::Search));
    state
        .catalog_checked
        .set(Some(Instant::now() - Duration::from_secs(5 * 60)));
    delegate.ensure_app_catalog();
    assert!(
        state.catalog_receiver.borrow().is_none(),
        "reuse a five-minute-old catalog"
    );

    let input =
        NSSearchField::initWithFrame(NSSearchField::alloc(mtm), rect(0.0, 0.0, 200.0, 28.0));
    for checked in [None, Some(Instant::now() - Duration::from_secs(11 * 60))] {
        state.catalog_checked.set(checked);
        state.query.borrow_mut().clear();
        input.setStringValue(ns_string!("browser"));
        let notification = unsafe {
            NSNotification::notificationWithName_object(
                ns_string!("NSControlTextDidChangeNotification"),
                Some(&input),
            )
        };
        unsafe {
            let _: () = msg_send![&*delegate, controlTextDidChange: &*notification];
        }
        assert!(
            state.catalog_receiver.borrow().is_some(),
            "typing must refresh a missing or expired catalog"
        );
        let apps = state
            .catalog_receiver
            .borrow_mut()
            .take()
            .unwrap()
            .recv_timeout(Duration::from_secs(15))
            .expect("catalog scan must complete");
        let (tx, rx) = mpsc::channel();
        state.catalog_receiver.replace(Some(rx));
        delegate.ensure_app_catalog();
        tx.send(apps).expect("a pending scan must not be replaced");
        delegate.poll_app_catalog();
        assert!(state.catalog_receiver.borrow().is_none());
        assert!(state.catalog_checked.get().unwrap().elapsed() < Duration::from_secs(5));
        delegate.filter();
        assert!(
            state.catalog_receiver.borrow().is_none(),
            "completed scans must be reused"
        );
    }
    // Notifications refresh an existing cache even while the search UI is closed.
    state.mode.set(None);
    let directory = tempfile::tempdir().unwrap();
    state.catalog_watcher.replace(Some(
        crate::macos::platform::catalog_watcher::CatalogWatcher::new(
            vec![directory.path().to_path_buf()],
            || {},
        )
        .unwrap(),
    ));
    std::fs::create_dir(directory.path().join("New.app")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    while state.catalog_generation.get() == 0 && Instant::now() < deadline {
        delegate.poll_app_catalog_changes();
        std::thread::sleep(Duration::from_millis(25));
    }
    state.catalog_watcher.take();
    assert!(state.catalog_checked.get().is_none());
    assert!(
        state.catalog_receiver.borrow().is_some(),
        "install notifications must refresh the cache in the background"
    );
    state.mode.set(Some(PanelMode::Search));
    state.query.replace("neomac".into());
    delegate.ensure_app_catalog();
    assert!(
        state.catalog_receiver.borrow().is_some(),
        "installation invalidates the ten-minute cache"
    );
    let scan = state
        .catalog_receiver
        .borrow_mut()
        .take()
        .unwrap()
        .recv_timeout(Duration::from_secs(15))
        .unwrap();
    let (tx, rx) = mpsc::channel();
    state.catalog_receiver.replace(Some(rx));
    let cached = state.installed_apps.borrow().clone();
    delegate.invalidate_app_catalog();
    tx.send(scan).unwrap();
    delegate.poll_app_catalog();
    assert_eq!(
        *state.installed_apps.borrow(),
        cached,
        "a superseded scan must not overwrite the cache"
    );
    assert!(state.catalog_checked.get().is_none());
    assert!(
        state.catalog_receiver.borrow().is_some(),
        "a change during a scan needs another scan"
    );
    let scan = state
        .catalog_receiver
        .borrow_mut()
        .take()
        .unwrap()
        .recv_timeout(Duration::from_secs(15))
        .unwrap();
    let (tx, rx) = mpsc::channel();
    state.catalog_receiver.replace(Some(rx));
    tx.send(scan).unwrap();
    delegate.poll_app_catalog();
    assert!(state.catalog_checked.get().is_some());
    assert_eq!(&*state.query.borrow(), "neomac");
    assert!(state.catalog_receiver.borrow().is_none());
    println!(
        "Catalog refresh checks passed: demand, scope, ten-minute fallback, install invalidation, in-flight races and query preservation."
    );
}

pub(super) fn verify_typo_search(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.catalog_checked.set(Some(Instant::now()));
    state.mode.set(Some(PanelMode::Search));
    state.windows.replace(vec![
        WindowInfo {
            id: 1,
            pid: -10,
            app: "Google Chrome".into(),
            title: "Documentation".into(),
            minimized: false,
        },
        WindowInfo {
            id: 2,
            pid: -20,
            app: "Code".into(),
            title: "channel_signer.rs — fiber".into(),
            minimized: false,
        },
    ]);
    state.installed_apps.replace(vec![InstalledApp {
        names: vec!["Obsidian".into()],
        target: ApplicationTarget {
            bundle_id: "com.example.notes".into(),
            name: "Obsidian".into(),
            path: "/Applications/Example Notes.app".into(),
        },
    }]);
    delegate.sync_displays();
    let input = delegate.panels()[0].input.clone();
    for (query, expected) in [
        ("chorme", SelectedResult::Window(1)),
        ("code fibre", SelectedResult::Window(2)),
        (
            "obsidxxn",
            SelectedResult::Application("com.example.notes".into()),
        ),
    ] {
        input.setStringValue(&NSString::from_str(query));
        let notification = unsafe {
            NSNotification::notificationWithName_object(
                ns_string!("NSControlTextDidChangeNotification"),
                Some(&input),
            )
        };
        unsafe {
            let _: () = msg_send![&*delegate, controlTextDidChange: &*notification];
        }
        match expected {
            SelectedResult::File(_)
            | SelectedResult::KeepAwake(_)
            | SelectedResult::Bluetooth(_)
            | SelectedResult::Project(_)
            | SelectedResult::OpenUrl(_)
            | SelectedResult::Quicklink(_)
            | SelectedResult::Snippet(_)
            | SelectedResult::Emoji(_)
            | SelectedResult::Clipboard(_) => panic!("unexpected snippet in launch test"),
            SelectedResult::Command(id) => assert_eq!(delegate.selected_command(), Some(id)),
            SelectedResult::Window(id) => {
                assert_eq!(delegate.selected_window().map(|window| window.id), Some(id));
            }
            SelectedResult::Application(id) => {
                assert_eq!(
                    delegate.selected_application().map(|app| app.bundle_id),
                    Some(id)
                );
            }
        }
        for ui in delegate.panels() {
            assert_eq!(ui.input.stringValue().to_string(), query);
            assert!(ui.rows.borrow()[0].button.isAccessibilitySelected());
            assert!(!ui.panel.isVisible());
        }
        assert!(state.catalog_receiver.borrow().is_none());
    }
    println!(
        "Typo search passed on all panels: app name, project word, and installed-app launch result."
    );
}

pub(super) fn verify_launch_search(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.catalog_checked.set(Some(Instant::now()));
    state.mode.set(Some(PanelMode::Search));
    state.identities.borrow_mut().insert(
        -100,
        AppIdentity {
            id: "com.example.browser".into(),
            english_name: "Browser".into(),
        },
    );
    state.installed_apps.replace(
        [
            ("com.example.browser", "Browser"),
            ("com.example.beta", "Browser Beta"),
            ("com.example.terminal", "Terminal"),
        ]
        .into_iter()
        .map(|(id, name)| InstalledApp {
            target: ApplicationTarget {
                bundle_id: id.into(),
                name: name.into(),
                path: format!("/Applications/{name}.app"),
            },
            names: vec![name.into()],
        })
        .collect(),
    );
    delegate.install_windows(vec![WindowInfo {
        id: 900,
        pid: -100,
        app: "Browser".into(),
        title: "Documentation".into(),
        minimized: false,
    }]);
    delegate.sync_displays();
    delegate.filter();
    assert_eq!(delegate.match_count(), 1, "empty search shows windows only");
    assert!(state.launch_matches.borrow().is_empty());
    state.query.replace("browser".into());
    delegate.filter();
    assert_eq!(state.matches.borrow().len(), 1);
    assert_eq!(state.launch_matches.borrow().len(), 1);
    delegate.move_selection(1);
    assert!(delegate.selected_window().is_none());
    assert_eq!(
        delegate.selected_application().unwrap().bundle_id,
        "com.example.beta"
    );
    for ui in delegate.panels() {
        let rows = ui.rows.borrow();
        assert_eq!(rows[1].title.stringValue().to_string(), "启动应用");
        assert_eq!(rows[1].alias.stringValue().to_string(), "↗");
        assert!(rows[1].button.isAccessibilitySelected());
        assert!(!ui.panel.isVisible());
    }
    if let Ok(directory) = std::env::var("WINLANE_PREVIEW_DIR") {
        std::fs::create_dir_all(&directory).unwrap();
        let ui = delegate.panels()[0].clone();
        // SAFETY: AppKit provides immutable appearance-name constants.
        let (name, appearance) = unsafe {
            if std::env::var_os("WINLANE_DARK_PREVIEW").is_some() {
                ("dark", NSAppearanceNameDarkAqua)
            } else {
                ("light", NSAppearanceNameAqua)
            }
        };
        {
            ui.panel
                .setAppearance(NSAppearance::appearanceNamed(appearance).as_deref());
            let root = ui.panel.contentView().unwrap();
            root.layoutSubtreeIfNeeded();
            let bitmap = root
                .bitmapImageRepForCachingDisplayInRect(root.bounds())
                .unwrap();
            root.cacheDisplayInRect_toBitmapImageRep(root.bounds(), &bitmap);
            let png = unsafe {
                bitmap.representationUsingType_properties(
                    NSBitmapImageFileType::PNG,
                    &objc2_foundation::NSDictionary::new(),
                )
            }
            .unwrap();
            assert!(png.writeToFile_atomically(
                &NSString::from_str(&format!("{directory}/{name}-search-launch.png")),
                true
            ));
        }
    }
    let (tx, rx) = mpsc::channel();
    let mut reordered = state.installed_apps.borrow().clone();
    reordered.reverse();
    state.catalog_receiver.replace(Some(rx));
    tx.send(reordered).unwrap();
    delegate.poll_app_catalog();
    assert_eq!(
        delegate.selected_application().unwrap().bundle_id,
        "com.example.beta",
        "catalog refresh must preserve selected identity"
    );
    let selected = delegate.selected_result();
    state.windows.borrow_mut().clear();
    delegate.filter_preserving(selected);
    assert_eq!(
        delegate.selected_application().unwrap().bundle_id,
        "com.example.beta",
        "window removal must not shift the selected app"
    );
    delegate.move_selection(1);
    assert_eq!(
        delegate.selected_application().unwrap().bundle_id,
        "com.example.browser"
    );
    state.query.replace("terminal".into());
    delegate.filter();
    assert_eq!(delegate.match_count(), 1);
    assert_eq!(
        delegate.selected_application().unwrap().bundle_id,
        "com.example.terminal"
    );
    state.current_app_only.set(true);
    delegate.filter();
    assert_eq!(delegate.match_count(), 0);
    state.current_app_only.set(false);
    state.query.replace("browser".into());
    state.identities.borrow_mut().insert(
        -101,
        AppIdentity {
            id: "com.example.beta".into(),
            english_name: "Browser Beta".into(),
        },
    );
    delegate.install_windows(vec![
        WindowInfo {
            id: 900,
            pid: -100,
            app: "Browser".into(),
            title: "Documentation".into(),
            minimized: false,
        },
        WindowInfo {
            id: 901,
            pid: -101,
            app: "Browser Beta".into(),
            title: "New window".into(),
            minimized: false,
        },
    ]);
    delegate.filter_preserving(Some(SelectedResult::Application("com.example.beta".into())));
    assert_eq!(
        delegate.selected_window().unwrap().id,
        901,
        "newly discovered window must retain the selected application"
    );
    assert!(state.launch_matches.borrow().is_empty());
    state.mode.set(Some(PanelMode::Switch));
    delegate.filter();
    assert!(state.launch_matches.borrow().is_empty());
    state.mode.set(Some(PanelMode::Search));
    state.demo.set(true);
    delegate.filter();
    assert!(
        state.launch_matches.borrow().is_empty(),
        "demo must not offer real application launches"
    );
    assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));
}

pub(crate) fn launch_search_fixture(target: ApplicationTarget) -> Receiver<Result<i32, String>> {
    let delegate = Delegate::new(MainThreadMarker::new().unwrap());
    let state = delegate.ivars();
    state.catalog_checked.set(Some(Instant::now()));
    state.mode.set(Some(PanelMode::Search));
    state.query.replace(target.name.clone());
    state.installed_apps.replace(vec![InstalledApp {
        names: vec![target.name.clone()],
        target,
    }]);
    delegate.filter();
    assert_eq!(delegate.match_count(), 1);
    assert!(delegate.selected_window().is_none());
    delegate.activate_selected();
    assert!(state.mode.get().is_none());
    let pending = state.launch_receiver.borrow_mut().take().unwrap();
    assert!(matches!(pending.origin, LaunchOrigin::Search));
    pending.receiver
}
