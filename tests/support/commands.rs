fn verify_command_search(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.catalog_checked.set(Some(Instant::now()));
    state.mode.set(Some(PanelMode::Search));
    state.installed_apps.replace(vec![InstalledApp {
        target: ApplicationTarget {
            bundle_id: "com.example.menu".into(),
            name: "Show Menu Helper".into(),
            path: "/Applications/Show Menu Helper.app".into(),
        },
        names: vec!["Show Menu Helper".into()],
    }]);
    delegate.install_windows(vec![WindowInfo {
        id: 700,
        pid: -700,
        app: "Editor".into(),
        title: "show-menu notes".into(),
        minimized: false,
    }]);
    delegate.sync_displays();
    delegate.filter();
    assert_eq!(delegate.match_count(), 1);
    assert!(state.command_matches.borrow().is_empty());

    state.query.replace("menu".into());
    delegate.filter();
    assert_eq!(delegate.match_count(), 3);
    assert_eq!(delegate.selected_command(), Some(CommandId::ShowMenu));
    assert!(delegate.selected_window().is_none());
    assert!(delegate.selected_application().is_none());
    for ui in delegate.panels() {
        delegate.remember_panel_display(&ui.panel);
        assert_eq!(state.keyboard_display.get(), Some(ui.display_id));
        let rows = ui.rows.borrow();
        assert_eq!(rows[0].app.stringValue().to_string(), "show-menu");
        assert_eq!(rows[0].title.stringValue().to_string(), "显示 macOS 菜单栏");
        assert_eq!(rows[0].alias.stringValue().to_string(), ">_");
        assert!(rows[0].icon.image().is_some());
        assert!(rows[0].button.isAccessibilitySelected());
        assert!(!ui.panel.isVisible());
    }

    delegate.move_selection(1);
    assert_eq!(delegate.selected_window().unwrap().id, 700);
    assert!(delegate.selected_command().is_none());
    assert!(delegate.selected_application().is_none());
    delegate.filter_preserving(delegate.selected_result());
    assert_eq!(delegate.selected_window().unwrap().id, 700);

    delegate.move_selection(1);
    assert_eq!(delegate.selected_application().unwrap().bundle_id, "com.example.menu");
    delegate.filter_preserving(delegate.selected_result());
    assert_eq!(delegate.selected_application().unwrap().bundle_id, "com.example.menu");
    delegate.move_selection(1);
    assert_eq!(delegate.selected_command(), Some(CommandId::ShowMenu));

    let (tx, rx) = mpsc::channel();
    state.catalog_receiver.replace(Some(rx));
    tx.send(Vec::new()).unwrap();
    delegate.poll_app_catalog();
    assert_eq!(delegate.match_count(), 2);
    assert_eq!(delegate.selected_command(), Some(CommandId::ShowMenu));
    delegate.install_windows(Vec::new());
    delegate.filter_preserving(delegate.selected_result());
    assert_eq!(delegate.match_count(), 1);
    assert_eq!(delegate.selected_command(), Some(CommandId::ShowMenu));

    state.mode.set(Some(PanelMode::Switch));
    delegate.filter();
    assert!(state.command_matches.borrow().is_empty());
    assert_eq!(delegate.match_count(), 0);
    state.mode.set(Some(PanelMode::Search));
    state.demo.set(true);
    delegate.filter();
    assert!(state.command_matches.borrow().is_empty());
    state.demo.set(false);
    state.current_app_only.set(true);
    delegate.filter();
    assert!(state.command_matches.borrow().is_empty());
    state.current_app_only.set(false);
    state.query.replace("rust".into());
    delegate.filter();
    assert!(delegate.selected_command().is_none());

    for command in [CommandId::LockScreen, CommandId::Sleep, CommandId::MissionControl, CommandId::Screenshot] {
        state.query.replace(command.definition().name.into());
        delegate.filter();
        assert_eq!(delegate.match_count(), 1);
        assert_eq!(delegate.selected_command(), Some(command));
        for ui in delegate.panels() {
            let rows = ui.rows.borrow();
            assert_eq!(rows[0].app.stringValue().to_string(), command.definition().name);
            assert_eq!(rows[0].title.stringValue().to_string(), command.definition().title());
            assert!(rows[0].icon.image().is_some());
            assert!(rows[0].button.isAccessibilitySelected());
        }
        delegate.filter_preserving(delegate.selected_result());
        assert_eq!(delegate.selected_command(), Some(command));
    }
    state.query.replace("show".into());
    delegate.filter();
    assert_eq!(delegate.match_count(), 1);
    assert_eq!(delegate.selected_command(), Some(CommandId::ShowMenu));
    delegate.filter_preserving(delegate.selected_result());
    assert_eq!(delegate.selected_command(), Some(CommandId::ShowMenu));
    assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));
}
