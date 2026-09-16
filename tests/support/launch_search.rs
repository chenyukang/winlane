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
