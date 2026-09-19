use super::*;

pub(super) fn verify_editor_window_titles(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.demo.set(true);
    state.mode.set(Some(PanelMode::Search));
    delegate.sync_displays();
    let cases = [
        (
            "com.microsoft.VSCode",
            "snapshot2.rs — ckb",
            "ckb: snapshot2.rs",
        ),
        (
            "com.microsoft.VSCode",
            "README.md (added in abc123) (README.md ((deleted)) ↔ README.md (Working Tree)) — project",
            "project: README.md (added in abc123) (README.md ((deleted)) ↔ README.md (Working Tree))",
        ),
        (
            "com.microsoft.VSCode",
            "● main.rs — my - project — Visual Studio Code",
            "my - project: ● main.rs",
        ),
        (
            "com.microsoft.VSCodeInsiders",
            "main.rs — project [SSH: dev] — Visual Studio Code - Insiders",
            "project [SSH: dev]: main.rs",
        ),
        (
            "com.microsoft.VSCode",
            "main.rs - project - Visual Studio Code",
            "project: main.rs",
        ),
        (
            "com.microsoft.VSCodeInsiders",
            "main.rs - project - Visual Studio Code - Insiders",
            "project: main.rs",
        ),
        (
            "com.microsoft.VSCode",
            "Welcome — Visual Studio Code",
            "Welcome — Visual Studio Code",
        ),
        ("com.microsoft.VSCode", "project", "project"),
        ("com.microsoft.VSCode", "main.rs — ", "main.rs — "),
        (
            "com.example.other",
            "snapshot2.rs — ckb",
            "snapshot2.rs — ckb",
        ),
    ];
    for (bundle, original, expected) in cases {
        state.identities.borrow_mut().insert(
            -100,
            AppIdentity {
                id: bundle.into(),
                english_name: "Code".into(),
            },
        );
        let window = WindowInfo {
            id: 100,
            pid: -100,
            app: "Code".into(),
            title: original.into(),
            minimized: false,
        };
        state.windows.replace(vec![window.clone()]);
        for mode in [PanelMode::Search, PanelMode::Switch] {
            state.mode.set(Some(mode));
            state.query.borrow_mut().clear();
            delegate.filter();
            assert_eq!(delegate.selected_window().unwrap(), window);
            for ui in delegate.panels() {
                assert_eq!(
                    ui.rows.borrow()[0].title.stringValue().to_string(),
                    expected
                );
                assert!(!ui.panel.isVisible());
            }
        }
    }
    state.identities.borrow_mut().get_mut(&-100).unwrap().id = "com.microsoft.VSCode".into();
    state.windows.borrow_mut()[0].minimized = true;
    state.mode.set(Some(PanelMode::Search));
    state.query.replace("snapshot2.rs ckb".into());
    delegate.filter();
    assert_eq!(
        delegate.match_count(),
        1,
        "search must still use the original title"
    );
    for ui in delegate.panels() {
        assert_eq!(
            ui.rows.borrow()[0].title.stringValue().to_string(),
            "ckb: snapshot2.rs · 已最小化"
        );
    }
    println!("VS Code project-first titles passed in search and switch panels.");
}

pub(super) fn verify_app_name_search(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.demo.set(true);
    state.mode.set(Some(PanelMode::Search));
    state.windows.replace(vec![
        WindowInfo {
            id: 1,
            pid: -1,
            app: "Code".into(),
            title: "localization.rs (Working Tree) (localization.rs) — windowlane".into(),
            minimized: false,
        },
        WindowInfo {
            id: 2,
            pid: -2,
            app: "Notion".into(),
            title: "CKB Dev Log".into(),
            minimized: false,
        },
    ]);
    state.recency.replace(vec![1, 2]);
    delegate.sync_displays();
    delegate.filter();
    assert_eq!(delegate.selected_window().unwrap().id, 1);
    for query in ["noti", "notion"] {
        state.query.replace(query.into());
        delegate.filter();
        assert_eq!(delegate.selected_window().unwrap().id, 2);
        assert_eq!(delegate.match_count(), 2);
        for ui in delegate.panels() {
            let rows = ui.rows.borrow();
            assert_eq!(rows[0].app.stringValue().to_string(), "Notion");
            assert!(rows[0].button.isAccessibilitySelected());
            assert_eq!(rows[1].app.stringValue().to_string(), "Code");
            assert!(!ui.panel.isVisible());
        }
    }
    println!(
        "Partial and exact app-name searches selected Notion ahead of a recent Code fuzzy match on all panels."
    );
}

pub(super) fn verify_alias_does_not_block_title_search(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.demo.set(true);
    state.mode.set(Some(PanelMode::Search));
    state.automatic_aliases.replace(
        Aliases::from_json(r#"{"com.apple.finder":"fi","com.microsoft.VSCode":"co"}"#).unwrap(),
    );
    state.identities.replace(HashMap::from([
        (
            -10,
            AppIdentity {
                id: "com.microsoft.VSCode".into(),
                english_name: "Code".into(),
            },
        ),
        (
            -20,
            AppIdentity {
                id: "com.apple.finder".into(),
                english_name: "Finder".into(),
            },
        ),
    ]));
    let fiber = WindowInfo {
        id: 1,
        pid: -10,
        app: "Code".into(),
        title: "channel.rs — fiber".into(),
        minimized: false,
    };
    let other = WindowInfo {
        id: 2,
        pid: -10,
        app: "Code".into(),
        title: "main.rs — rust".into(),
        minimized: false,
    };
    let finder = WindowInfo {
        id: 3,
        pid: -20,
        app: "Finder".into(),
        title: "Documents".into(),
        minimized: false,
    };
    delegate.install_windows(vec![other.clone(), fiber.clone()]);
    delegate.sync_displays();
    for query in ["f", "fi", "fib", "fiber", " FI "] {
        state.query.replace(query.into());
        delegate.filter();
        assert_eq!(
            delegate.match_count(),
            1,
            "a saved alias without a window must not block {query}"
        );
        assert_eq!(delegate.selected_window().unwrap().id, 1);
    }
    state.query.replace("fi".into());
    delegate.install_windows(vec![other.clone(), fiber.clone(), finder]);
    delegate.filter();
    assert_eq!(
        delegate.match_count(),
        2,
        "a live alias must retain the matching project below it"
    );
    assert_eq!(delegate.selected_window().unwrap().id, 3);
    delegate.move_selection(1);
    assert_eq!(delegate.selected_window().unwrap().id, 1);
    delegate.filter_preserving(delegate.selected_result());
    assert_eq!(delegate.selected_window().unwrap().id, 1);
    for ui in delegate.panels() {
        let rows = ui.rows.borrow();
        assert_eq!(rows[1].title.stringValue().to_string(), "fiber: channel.rs");
        assert!(rows[1].button.isAccessibilitySelected());
        assert!(!ui.panel.isVisible());
    }
    state.config.borrow_mut().excluded_apps = vec!["Finder".into()];
    delegate.filter();
    assert_eq!(delegate.match_count(), 1);
    assert_eq!(delegate.selected_window().unwrap().id, 1);
    state.config.borrow_mut().excluded_apps = vec!["Code".into()];
    delegate.filter();
    assert_eq!(delegate.match_count(), 1);
    assert_eq!(delegate.selected_window().unwrap().id, 3);
    state.config.borrow_mut().excluded_apps.clear();
    delegate.install_windows(vec![other, fiber]);
    delegate.filter();
    assert_eq!(delegate.match_count(), 1);
    assert_eq!(delegate.selected_window().unwrap().id, 1);
    println!(
        "Alias/title search checks passed: fi finds fiber with Finder absent, present, excluded, and closed; selection survives refresh."
    );
}

pub(super) fn verify_distinct_window_aliases(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.demo.set(true);
    state
        .automatic_aliases
        .replace(Aliases::from_json(r#"{"code":"co"}"#).unwrap());
    state.identities.borrow_mut().insert(
        -42,
        AppIdentity {
            id: "code".into(),
            english_name: "Code".into(),
        },
    );
    let windows: Vec<_> = (1..=3)
        .map(|id| WindowInfo {
            id,
            pid: -42,
            app: "Code".into(),
            title: format!("Project {id}"),
            minimized: false,
        })
        .collect();
    delegate.update_aliases(&windows);
    state.windows.replace(windows);
    state.mode.set(Some(PanelMode::Search));
    delegate.sync_displays();
    delegate.filter();
    let expected: Vec<_> = delegate.panels()[0]
        .rows
        .borrow()
        .iter()
        .map(|row| {
            let RowContent::Window(window, _) = row.content.as_ref().unwrap() else {
                panic!("expected a window")
            };
            (window.id, row.alias.stringValue().to_string())
        })
        .collect();
    assert_eq!(expected.len(), 3);
    assert!(expected.iter().all(|(_, alias)| !alias.is_empty()));
    assert_eq!(
        expected
            .iter()
            .map(|(_, alias)| alias)
            .collect::<HashSet<_>>()
            .len(),
        3,
        "each independent Code window needs a distinct alias"
    );
    for (id, alias) in &expected {
        state.mode.set(Some(PanelMode::Search));
        state.recency.replace(vec![3, 1, 2]);
        state.query.replace(alias.clone());
        delegate.filter();
        assert_eq!(state.matches.borrow().len(), 3);
        assert_eq!(delegate.selected_window().unwrap().id, *id);
        let mut ordered_ids = vec![*id];
        ordered_ids.extend([3, 1, 2].into_iter().filter(|other| other != id));
        assert_eq!(
            state
                .matches
                .borrow()
                .iter()
                .map(|&index| state.windows.borrow()[index].id)
                .collect::<Vec<_>>(),
            ordered_ids,
            "the exact alias leads, followed by sibling windows in recent order"
        );
        for ui in delegate.panels() {
            let rows = ui.rows.borrow();
            assert!(rows[0].button.isAccessibilitySelected());
            assert!(rows.iter().take(3).all(|row| row.attached));
        }
        delegate.move_selection(1);
        assert_eq!(delegate.selected_window().unwrap().id, ordered_ids[1]);
        delegate.filter_preserving(delegate.selected_result());
        assert_eq!(delegate.selected_window().unwrap().id, ordered_ids[1]);
        state.query.borrow_mut().clear();
        state.mode.set(Some(PanelMode::Switch));
        state
            .switch_selection
            .replace(Some(SwitchSelection::new(0)));
        state.alias_input.borrow_mut().clear();
        delegate.filter();
        delegate.prepare_switch_selection();
        for ch in alias.chars() {
            delegate.shortcut_action(Action {
                session: state.session.get(),
                kind: ActionKind::Alias(ch),
            });
        }
        assert_eq!(delegate.selected_window().unwrap().id, *id);
        for ui in delegate.panels() {
            let row = &ui.rows.borrow()[state.selected.get()];
            assert_eq!(row.alias.stringValue().to_string(), *alias);
            assert!(row.button.isAccessibilitySelected());
        }
        delegate.shortcut_action(Action {
            session: state.session.get(),
            kind: ActionKind::Accept,
        });
        assert!(delegate.panels().iter().all(|ui| {
            ui.footer
                .stringValue()
                .to_string()
                .contains(&format!("Project {id}（"))
        }));
    }

    state.mode.set(Some(PanelMode::Switch));
    state
        .switch_selection
        .replace(Some(SwitchSelection::new(0)));
    state.alias_input.borrow_mut().clear();
    delegate.filter();
    delegate.prepare_switch_selection();
    let before = state.aliases.borrow().clone();
    let mut pending = state.windows.borrow().clone();
    pending.retain(|window| window.id != 1);
    pending.push(WindowInfo {
        id: 4,
        pid: -42,
        app: "Code".into(),
        title: "New project".into(),
        minimized: false,
    });
    state.deferred_windows.replace(Some(pending));
    delegate.render();
    assert_eq!(*state.aliases.borrow(), before);
    delegate.display_search(state.session.get());
    let after = state.aliases.borrow();
    assert_eq!(after.for_window(1), None);
    assert!(after.for_window(4).is_some());
    for id in [2, 3] {
        assert_eq!(after.for_window(id), before.for_window(id));
    }
}

pub(super) fn verify_switch_alias_prefix(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.demo.set(true);
    state.session.set(7);
    state
        .automatic_aliases
        .replace(Aliases::from_json(r#"{"zed":"z","zulip":"zu","zoom":"zo"}"#).unwrap());
    state.windows.replace(vec![
        WindowInfo {
            id: 41,
            pid: -41,
            app: "Code".into(),
            title: "Project".into(),
            minimized: false,
        },
        WindowInfo {
            id: 42,
            pid: -42,
            app: "Zulip".into(),
            title: "Messages".into(),
            minimized: false,
        },
    ]);
    state.identities.borrow_mut().insert(
        -42,
        AppIdentity {
            id: "zulip".into(),
            english_name: "Zulip".into(),
        },
    );
    delegate.update_aliases(&state.windows.borrow());
    state.mode.set(Some(PanelMode::Switch));
    state
        .switch_selection
        .replace(Some(SwitchSelection::new(0)));
    delegate.sync_displays();
    delegate.filter();
    delegate.prepare_switch_selection();
    delegate.shortcut_action(Action {
        session: 7,
        kind: ActionKind::Alias('z'),
    });
    assert_eq!(delegate.selected_window().unwrap().id, 42);
    for ui in delegate.panels() {
        assert!(!ui.mode_label.stringValue().to_string().contains("没有匹配"));
        assert!(
            ui.rows.borrow()[state.selected.get()]
                .button
                .isAccessibilitySelected()
        );
    }
    delegate.shortcut_action(Action {
        session: 7,
        kind: ActionKind::Accept,
    });
    assert_eq!(state.mode.get(), Some(PanelMode::Search));
    assert!(delegate.panels().iter().all(|ui| {
        ui.footer
            .stringValue()
            .to_string()
            .contains("演示选择：Zulip")
    }));

    state.windows.borrow_mut().push(WindowInfo {
        id: 43,
        pid: -43,
        app: "Zoom".into(),
        title: "Meeting".into(),
        minimized: false,
    });
    state.identities.borrow_mut().insert(
        -43,
        AppIdentity {
            id: "zoom".into(),
            english_name: "Zoom".into(),
        },
    );
    delegate.update_aliases(&state.windows.borrow());
    state.mode.set(Some(PanelMode::Switch));
    state
        .switch_selection
        .replace(Some(SwitchSelection::new(0)));
    delegate.filter();
    delegate.prepare_switch_selection();
    delegate.shortcut_action(Action {
        session: 7,
        kind: ActionKind::Alias('z'),
    });
    assert_eq!(delegate.alias_match(), AliasMatch::Ambiguous);
    for ui in delegate.panels() {
        assert!(
            ui.mode_label
                .stringValue()
                .to_string()
                .contains("继续输入第二个字母")
        );
        assert!(
            ui.rows
                .borrow()
                .iter()
                .all(|row| !row.button.isAccessibilitySelected())
        );
    }
    delegate.shortcut_action(Action {
        session: 7,
        kind: ActionKind::Alias('u'),
    });
    assert_eq!(delegate.selected_window().unwrap().id, 42);
    delegate.shortcut_action(Action {
        session: 7,
        kind: ActionKind::AliasBackspace,
    });
    assert_eq!(delegate.alias_match(), AliasMatch::Ambiguous);
    delegate.shortcut_action(Action {
        session: 7,
        kind: ActionKind::Accept,
    });
    assert_eq!(
        state.mode.get(),
        None,
        "ambiguous release must cancel instead of committing the previous selection"
    );
    assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));
}
