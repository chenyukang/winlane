fn verify_shortcut_recency(mtm: MainThreadMarker) {
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

fn verify_catalog_refresh(mtm: MainThreadMarker) {
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
    println!(
        "Catalog refresh checks passed: demand, scope, ten-minute cache, in-flight reuse, completion."
    );
}

fn verify_launch_search(mtm: MainThreadMarker) {
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

pub fn launch_search_fixture(target: ApplicationTarget) -> Receiver<Result<i32, String>> {
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
