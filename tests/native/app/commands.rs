use super::*;

pub(super) fn verify_command_search(mtm: MainThreadMarker) {
    verify_command_shortcut_scope(mtm);
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
    assert_eq!(
        delegate.selected_application().unwrap().bundle_id,
        "com.example.menu"
    );
    delegate.filter_preserving(delegate.selected_result());
    assert_eq!(
        delegate.selected_application().unwrap().bundle_id,
        "com.example.menu"
    );
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

    for command in [
        CommandId::LockScreen,
        CommandId::Sleep,
        CommandId::MissionControl,
        CommandId::Screenshot,
        CommandId::ToggleAppearance,
    ] {
        state.query.replace(command.definition().name.into());
        delegate.filter();
        assert_eq!(delegate.match_count(), 1);
        assert_eq!(delegate.selected_command(), Some(command));
        for ui in delegate.panels() {
            let rows = ui.rows.borrow();
            assert_eq!(
                rows[0].app.stringValue().to_string(),
                command.definition().name
            );
            assert_eq!(
                rows[0].title.stringValue().to_string(),
                command.definition().title()
            );
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
    // The date command renders the live date and time instead of a static title.
    state.query.replace("date".into());
    delegate.filter();
    assert_eq!(delegate.selected_command(), Some(CommandId::Date));
    for ui in delegate.panels() {
        let rows = ui.rows.borrow();
        assert_eq!(rows[0].app.stringValue().to_string(), "date");
        let shown = rows[0].title.stringValue().to_string();
        assert!(
            shown.contains('-')
                && shown.contains(':')
                && shown != CommandId::Date.definition().title(),
            "date row should show a timestamp: {shown}"
        );
    }
    assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));
}

fn verify_command_shortcut_scope(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.demo.set(true);
    state.catalog_checked.set(Some(Instant::now()));
    for (command, scope) in [
        (CommandId::KeepAwake, SearchScope::KeepAwake),
        (CommandId::OpenUrl, SearchScope::OpenUrl),
        (CommandId::Bluetooth, SearchScope::Bluetooth),
        (CommandId::Projects, SearchScope::Projects),
        (CommandId::Quicklinks, SearchScope::Quicklinks),
        (CommandId::Snippets, SearchScope::Snippets),
        (CommandId::Emoji, SearchScope::Emoji),
        (CommandId::Clipboard, SearchScope::Clipboard),
    ] {
        assert!(super::super::commands::command_scope(command) == Some(scope));
    }
    for (command, policy) in [
        (CommandId::Quicklinks, InputMethod::English),
        (CommandId::Quicklinks, InputMethod::Chinese),
        (CommandId::Snippets, InputMethod::Current),
        (CommandId::Snippets, InputMethod::LastUsed),
        (CommandId::Emoji, InputMethod::English),
        (CommandId::Emoji, InputMethod::Chinese),
    ] {
        state.config.borrow_mut().input_rules =
            winlane::features::input_rules::Settings::for_winlane(policy);
        let expected = input_source::preferred(policy, mtm).and_then(|source| source.id());
        state.mode.set(Some(PanelMode::Switch));
        state.query.replace("old query".into());
        assert!(delegate.prepare_command_search(command, 99));
        assert_eq!(state.mode.get(), Some(PanelMode::Search));
        assert_eq!(state.session.get(), 99);
        assert!(state.query.borrow().is_empty());
        assert!(state.switch_selection.borrow().is_none());
        assert!(state.search_scope.get() == super::super::commands::command_scope(command));
        assert_eq!(state.input_gate.borrow().target(), expected.as_deref());
        assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));
        delegate.end_session();
    }
    for command in winlane::core::commands::COMMANDS.iter().map(|c| c.id) {
        if super::super::commands::command_scope(command).is_none() {
            assert!(!delegate.prepare_command_search(command, 100));
            assert!(state.mode.get().is_none());
        }
        delegate.run_command_shortcut(command, 101);
        assert!(
            state.mode.get().is_none(),
            "removed bindings must not run queued command actions"
        );
    }
}
