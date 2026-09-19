use super::*;
use crate::macos::app::projects::ProjectUpdate;

pub(super) fn verify_scoped_loading(mtm: MainThreadMarker) {
    verify_scope_backspace(mtm);
    verify_scope_escape(mtm);
    use winlane::features::open_url::Page;
    use winlane::features::projects::{Kind, Project};

    for command in [CommandId::Projects, CommandId::OpenUrl] {
        for cached in [false, true] {
            let delegate = responsive_fixture(mtm);
            let state = delegate.ivars();
            if cached {
                state.project_cache.borrow_mut().projects = vec![Project {
                    path: "/example/project".into(),
                    name: "Cached project".into(),
                    kind: Kind::Folder,
                }];
                state.open_url_history.borrow_mut().pages = vec![Page {
                    url: "https://example.com".into(),
                    title: "Cached URL".into(),
                    last_visit_time: 1,
                }];
            }
            let before = state.render_passes.get();
            // No pending worker is supplied: preparing the first frame must
            // leave discovery and database reads for a later run-loop turn.
            assert!(delegate.prepare_command_search(command, 42));
            assert_eq!(delegate.match_count(), usize::from(cached));
            assert_eq!(state.render_passes.get() - before, delegate.panels().len());
            assert!(state.project_receiver.borrow().is_none());
            assert!(state.open_url_receiver.borrow().is_none());
            assert!(delegate.panels().iter().all(|ui| !ui.scope_back.isHidden()));
            if cached {
                assert!(delegate.panels().iter().all(|ui| {
                    matches!(
                        &ui.rows.borrow()[0].content,
                        Some(RowContent::Project(_)) | Some(RowContent::OpenUrl(_))
                    )
                }));
            } else {
                assert!(delegate.panels().iter().all(|ui| {
                    ui.empty_labels.borrow().iter().any(|label| {
                        label
                            .stringValue()
                            .to_string()
                            .contains(tr!("正在读取", "Loading"))
                    })
                }));
            }
            let timer = state
                .scoped_refresh_timer
                .borrow()
                .as_ref()
                .unwrap()
                .clone();
            delegate.end_session();
            assert!(!timer.isValid());
            let before = state.render_passes.get();
            delegate.refresh_search_scope(&timer);
            assert_eq!(state.render_passes.get(), before, "ignore stale callbacks");
            assert!(state.project_receiver.borrow().is_none());
            assert!(state.open_url_receiver.borrow().is_none());
            assert_eq!(
                state.project_cache.borrow().projects.len(),
                usize::from(cached)
            );
            assert_eq!(
                state.open_url_history.borrow().pages.len(),
                usize::from(cached)
            );
            assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));
        }
    }
    verify_deferred_refresh(mtm);
    verify_incremental_projects(mtm);
}

fn verify_scope_backspace(mtm: MainThreadMarker) {
    for command in [
        CommandId::Bluetooth,
        CommandId::Projects,
        CommandId::OpenUrl,
        CommandId::Quicklinks,
        CommandId::Snippets,
        CommandId::Clipboard,
    ] {
        let delegate = responsive_fixture(mtm);
        delegate.prepare_command_search(command, 45);
        let state = delegate.ivars();
        let scope = state.search_scope.get();
        let ui = delegate.panels()[0].clone();
        let editor = NSTextView::initWithFrame(NSTextView::alloc(mtm), rect(0.0, 0.0, 100.0, 30.0));
        for query in ["example", "", "", ""] {
            state.query.replace(query.into());
            delegate.filter();
            assert!(
                !delegate
                    .text_command(
                        sel!(control:textView:doCommandBySelector:),
                        &ui.input,
                        &editor,
                        sel!(deleteBackward:),
                    )
                    .as_bool(),
                "Backspace must remain native text editing in {command:?}"
            );
            assert!(state.search_scope.get() == scope);
            assert_eq!(state.mode.get(), Some(PanelMode::Search));
            assert_eq!(state.query.borrow().as_str(), query);
        }
        delegate.scope_back(sel!(leaveScopedSearch:), None);
        assert!(!delegate.scoped_search());
        assert_eq!(state.query.borrow().as_str(), command.definition().name);
        delegate.end_session();
    }
}

fn verify_scope_escape(mtm: MainThreadMarker) {
    for command in [
        CommandId::Bluetooth,
        CommandId::Projects,
        CommandId::OpenUrl,
        CommandId::Quicklinks,
        CommandId::Snippets,
        CommandId::Clipboard,
    ] {
        for query in ["", "example"] {
            for native_event in [false, true] {
                let delegate = responsive_fixture(mtm);
                delegate.prepare_command_search(command, 45);
                delegate.ivars().query.replace(query.into());
                delegate.filter();
                let ui = delegate.panels()[0].clone();
                if native_event {
                    let event = {
                        NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                            NSEventType::KeyDown, NSPoint::new(0.0, 0.0), NSEventModifierFlags::empty(),
                            0.0, ui.panel.windowNumber(), None, ns_string!("\u{1b}"), ns_string!("\u{1b}"), false, 53,
                        )
                    }.unwrap();
                    ui.panel.sendEvent(&event);
                } else {
                    let editor = NSTextView::initWithFrame(
                        NSTextView::alloc(mtm),
                        rect(0.0, 0.0, 100.0, 30.0),
                    );
                    unsafe {
                        NSTextInputClient::setMarkedText_selectedRange_replacementRange(
                            &*editor,
                            ns_string!("zhong"),
                            objc2_foundation::NSRange::new(5, 0),
                            objc2_foundation::NSRange::new(
                                objc2_foundation::NSNotFound as usize,
                                0,
                            ),
                        );
                    }
                    assert!(
                        !delegate
                            .text_command(
                                sel!(control:textView:doCommandBySelector:),
                                &ui.input,
                                &editor,
                                sel!(cancelOperation:)
                            )
                            .as_bool()
                    );
                    assert!(
                        delegate.scoped_search(),
                        "IME cancellation must not dismiss the command"
                    );
                    NSTextInputClient::unmarkText(&*editor);
                    assert!(
                        delegate
                            .text_command(
                                sel!(control:textView:doCommandBySelector:),
                                &ui.input,
                                &editor,
                                sel!(cancelOperation:)
                            )
                            .as_bool()
                    );
                }
                assert_eq!(
                    delegate.ivars().mode.get(),
                    None,
                    "Escape must close {command:?} in one step"
                );
                assert!(delegate.ivars().search_scope.get().is_none());
                assert!(delegate.ivars().scoped_refresh_timer.borrow().is_none());
                assert!(!delegate.any_panel_visible());
            }
        }
    }
}

fn verify_incremental_projects(mtm: MainThreadMarker) {
    use winlane::features::projects::{Cache, Kind, MAX_PROJECTS, MAX_RESULTS, Project};
    let delegate = responsive_fixture(mtm);
    let state = delegate.ivars();
    state.config.borrow_mut().show_usage_hints = false;
    let projects: Vec<_> = (0..MAX_PROJECTS)
        .map(|index| Project {
            path: format!("/example/project-{index:04}").into(),
            name: format!("Project {index:04}"),
            kind: Kind::Folder,
        })
        .collect();
    let (tx, rx) = mpsc::channel();
    state.project_receiver.replace(Some(rx));
    delegate.prepare_command_search(CommandId::Projects, 43);
    assert!(
        delegate
            .panels()
            .iter()
            .all(|ui| !ui.project_progress.isHidden())
    );
    assert_eq!(delegate.match_count(), 0);

    tx.send(ProjectUpdate::Cached(projects[..MAX_RESULTS].to_vec()))
        .unwrap();
    delegate.poll_projects();
    assert_eq!(delegate.match_count(), MAX_RESULTS);
    assert!(
        state.project_receiver.borrow().is_some(),
        "cached results must not end the ongoing refresh"
    );
    delegate.move_selection(3);
    let selection = delegate.selected_project();
    let mut full = Cache::default();
    full.projects = projects.clone();
    full.projects.swap(0, 3);
    tx.send(ProjectUpdate::Refreshed(full)).unwrap();
    delegate.poll_projects();
    assert_eq!(delegate.selected_project(), selection);
    assert!(state.project_receiver.borrow().is_none());
    assert!(
        delegate
            .panels()
            .iter()
            .all(|ui| ui.project_progress.isHidden() && ui.rows.borrow().len() == MAX_RESULTS)
    );

    let (tx, rx) = mpsc::channel();
    state.project_receiver.replace(Some(rx));
    state.query.replace("0499".into());
    delegate.filter();
    assert_eq!(
        delegate.selected_project(),
        Some(projects[499].clone()),
        "search is not restricted to the saved 25 projects"
    );
    assert!(
        delegate
            .panels()
            .iter()
            .all(|ui| !ui.project_progress.isHidden())
    );
    drop(tx);
    delegate.poll_projects();
    assert_eq!(
        delegate.selected_project(),
        Some(projects[499].clone()),
        "failed refresh preserves cached results and input"
    );
    assert_eq!(&*state.query.borrow(), "0499");
    assert!(state.project_cache.borrow().error.is_some());
    assert!(
        delegate
            .panels()
            .iter()
            .all(|ui| ui.project_progress.isHidden())
    );

    delegate.end_session();
    delegate.prepare_command_search(CommandId::Projects, 44);
    assert_eq!(delegate.match_count(), MAX_RESULTS);
    assert!(
        delegate
            .panels()
            .iter()
            .all(|ui| ui.rows.borrow().len() == MAX_RESULTS)
    );
    assert!(
        state.project_receiver.borrow().is_none(),
        "reopening renders cached rows before starting discovery"
    );
    delegate.end_session();
    assert!(
        delegate
            .panels()
            .iter()
            .all(|ui| ui.project_progress.isHidden())
    );

    let (tx, rx) = mpsc::channel();
    state.project_receiver.replace(Some(rx));
    tx.send(ProjectUpdate::Cached(projects[..MAX_RESULTS].to_vec()))
        .unwrap();
    tx.send(ProjectUpdate::Refreshed(Cache::default())).unwrap();
    let renders = state.render_passes.get();
    delegate.poll_projects();
    assert!(state.project_cache.borrow().projects.is_empty());
    assert_eq!(
        state.render_passes.get(),
        renders,
        "late loading results do not reopen a dismissed panel"
    );
    assert!(!delegate.any_panel_visible());
    println!(
        "Project loading: pending workers leave input usable; cached and refreshed phases preserve selection; 500 projects create at most 25 result rows per display."
    );
}

fn verify_deferred_refresh(mtm: MainThreadMarker) {
    let delegate = responsive_fixture(mtm);
    let state = delegate.ivars();
    delegate.prepare_command_search(CommandId::Projects, 1);
    let stale = state
        .scoped_refresh_timer
        .borrow()
        .as_ref()
        .unwrap()
        .clone();
    delegate.leave_scoped_search();
    assert!(!stale.isValid());
    delegate.enter_scoped_search(SearchScope::OpenUrl);
    let timer = state
        .scoped_refresh_timer
        .borrow()
        .as_ref()
        .unwrap()
        .clone();
    delegate.refresh_search_scope(&stale);
    assert!(timer.isValid());
    assert!(state.project_receiver.borrow().is_none());

    // Keep the worker pending through timer dispatch, explicit refresh, and
    // leaving/re-entering the command; none may replace the in-flight receiver.
    let (tx, rx) = mpsc::channel();
    state.open_url_receiver.replace(Some(rx));
    timer.fire();
    assert!(!timer.isValid());
    assert!(state.scoped_refresh_timer.borrow().is_none());
    delegate.refresh_open_url();
    delegate.leave_scoped_search();
    delegate.enter_scoped_search(SearchScope::OpenUrl);
    assert!(state.scoped_refresh_timer.borrow().is_none());
    tx.send(winlane::features::open_url::History::default())
        .unwrap();
    delegate.poll_open_url();
    assert!(state.open_url_receiver.borrow().is_none());
    assert!(state.project_receiver.borrow().is_none());

    delegate.enter_scoped_search(SearchScope::Projects);
    let timer = state
        .scoped_refresh_timer
        .borrow()
        .as_ref()
        .unwrap()
        .clone();
    let (tx, rx) = mpsc::channel();
    state.project_receiver.replace(Some(rx));
    timer.fire();
    assert!(!timer.isValid());
    delegate.refresh_projects(true);
    delegate.leave_scoped_search();
    delegate.enter_scoped_search(SearchScope::Projects);
    assert!(state.scoped_refresh_timer.borrow().is_none());
    tx.send(ProjectUpdate::Refreshed(
        winlane::features::projects::Cache::default(),
    ))
    .unwrap();
    delegate.poll_projects();
    assert!(state.project_receiver.borrow().is_none());
    delegate.end_session();
}
