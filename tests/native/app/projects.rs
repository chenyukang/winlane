use super::*;
use crate::macos::app::projects::ProjectUpdate;

pub(super) fn verify_projects_search(mtm: MainThreadMarker) {
    verify_project_open_cancellation(mtm);
    use winlane::features::projects::{Cache, Kind, Project};
    let temporary =
        std::env::temp_dir().join(format!("winlane-project-open-{}", std::process::id()));
    std::fs::create_dir(&temporary).unwrap();
    let folder = temporary.join("Example 世界 # %");
    std::fs::create_dir(&folder).unwrap();
    let workspace = temporary.join("Example workspace.code-workspace");
    std::fs::write(&workspace, r#"{"folders":[]}"#).unwrap();
    let available = crate::macos::platform::project_open::application_path().is_some();
    for (path, kind) in [(folder, Kind::Folder), (workspace, Kind::Workspace)] {
        let project = Project {
            path,
            name: "Example".into(),
            kind,
        };
        let prepared = crate::macos::platform::project_open::PreparedProject::new(&project);
        if available {
            let prepared = prepared.unwrap();
            assert!(prepared.project.isFileURL());
            assert_eq!(
                prepared.project.path().unwrap().to_string(),
                project.path.to_string_lossy(),
                "native URL preparation must preserve spaces, Unicode and percent signs"
            );
        } else {
            assert!(
                prepared.is_err(),
                "a missing VS Code installation must be reported"
            );
        }
        // Preparation only: never send the open request during automated tests.
    }
    std::fs::remove_dir_all(&temporary).unwrap();
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.catalog_checked.set(Some(Instant::now()));
    state.mode.set(Some(PanelMode::Search));
    delegate.sync_displays();
    let ui = delegate.panels()[0].clone();
    let windows_before = NSApplication::sharedApplication(mtm).windows().len();
    let projects = vec![
        Project {
            path: "/example/Recent 世界".into(),
            name: "Recent 世界".into(),
            kind: Kind::Folder,
        },
        Project {
            path: "/example/rust/my-workspace.code-workspace".into(),
            name: "my-workspace".into(),
            kind: Kind::Workspace,
        },
    ];
    state.project_cache.borrow_mut().projects = projects.clone();
    state.query.replace("".into());
    delegate.filter();
    assert!(state.project_matches.borrow().is_empty());
    assert!(
        state.project_receiver.borrow().is_none(),
        "ordinary search must not read VS Code data"
    );
    state.query.replace("projects".into());
    delegate.filter();
    assert_eq!(delegate.selected_command(), Some(CommandId::Projects));
    assert!(state.project_matches.borrow().is_empty());
    assert!(
        state.project_receiver.borrow().is_none(),
        "matching the command must not read VS Code data"
    );
    // Keep all data synthetic; simulate a worker completion without reading personal history.
    let (tx, rx) = mpsc::channel();
    state.project_receiver.replace(Some(rx));
    delegate.activate_selected();
    assert!(delegate.searching_projects());
    assert_eq!(delegate.match_count(), 2);
    assert_eq!(delegate.selected_project(), Some(projects[0].clone()));
    assert!(state.matches.borrow().is_empty());
    assert!(state.launch_matches.borrow().is_empty());
    assert!(state.command_matches.borrow().is_empty());
    assert!(state.quicklink_matches.borrow().is_empty());
    assert!(delegate.selected_quicklink().is_none());
    assert!(delegate.selected_window().is_none());
    for panel in delegate.panels() {
        assert!(!panel.scope_back.isHidden());
        assert_eq!(
            panel.rows.borrow()[0].app.stringValue().to_string(),
            projects[0].name
        );
        assert_eq!(
            panel.rows.borrow()[0].title.stringValue().to_string(),
            projects[0].path.to_string_lossy()
        );
        assert!(matches!(
            panel.rows.borrow()[0].content,
            Some(RowContent::Project(_))
        ));
    }
    delegate.move_selection(1);
    let mut cache = Cache::default();
    cache.projects = projects.clone();
    cache.projects.reverse();
    tx.send(ProjectUpdate::Refreshed(cache)).unwrap();
    delegate.poll_projects();
    assert_eq!(
        delegate.selected_project(),
        Some(projects[1].clone()),
        "selection should survive background refresh"
    );
    state.catalog_checked.set(None);
    delegate.ensure_app_catalog();
    assert!(state.catalog_receiver.borrow().is_none());
    state.catalog_checked.set(Some(Instant::now()));
    state.query.replace("世界".into());
    delegate.filter();
    assert_eq!(delegate.match_count(), 1);
    assert_eq!(delegate.selected_project(), Some(projects[0].clone()));
    delegate.toggle_mode(0);
    assert!(delegate.searching_projects());
    state.config.borrow_mut().show_usage_hints = false;
    delegate.activate_selected();
    assert!(
        delegate.searching_projects(),
        "missing project paths must leave search open"
    );
    assert!(state.project_cache.borrow().error.is_some());
    assert!(
        !ui.footer.isHidden(),
        "open errors must remain visible with hints off"
    );
    assert!(
        ui.footer
            .stringValue()
            .to_string()
            .contains(tr!("项目路径不可用", "Project unavailable"))
    );
    state.query.replace("no matching project".into());
    delegate.filter();
    delegate.activate_selected();
    assert!(delegate.searching_projects());
    assert_eq!(delegate.match_count(), 0);
    delegate.leave_scoped_search();
    assert!(!delegate.scoped_search());
    assert_eq!(state.query.borrow().as_str(), "projects");
    assert_eq!(delegate.selected_command(), Some(CommandId::Projects));
    assert!(state.project_matches.borrow().is_empty());
    let (tx, rx) = mpsc::channel();
    state.project_receiver.replace(Some(rx));
    delegate.activate_selected();
    state.query.borrow_mut().clear();
    let editor = NSTextView::initWithFrame(NSTextView::alloc(mtm), rect(0.0, 0.0, 100.0, 30.0));
    assert!(
        !delegate
            .text_command(
                sel!(control:textView:doCommandBySelector:),
                &ui.input,
                &editor,
                sel!(deleteBackward:)
            )
            .as_bool()
    );
    assert!(delegate.searching_projects());
    delegate.leave_scoped_search();
    let mut cache = Cache::default();
    cache.projects = projects.clone();
    tx.send(ProjectUpdate::Refreshed(cache)).unwrap();
    let before = state.render_passes.get();
    delegate.poll_projects();
    assert_eq!(
        delegate.selected_command(),
        Some(CommandId::Projects),
        "late completion must not reopen the project list"
    );
    assert_eq!(state.render_passes.get(), before);
    assert_eq!(state.project_cache.borrow().projects, projects);
    assert!(state.project_matches.borrow().is_empty());
    let (_tx, rx) = mpsc::channel();
    state.project_receiver.replace(Some(rx));
    delegate.activate_selected();
    assert_eq!(delegate.match_count(), 2);
    assert!(
        delegate
            .panels()
            .iter()
            .all(|panel| !panel.rows.borrow().is_empty())
    );
    delegate.end_session();
    assert!(state.project_matches.borrow().is_empty());
    assert!(
        delegate
            .panels()
            .iter()
            .all(|panel| panel.rows.borrow().is_empty())
    );
    assert_eq!(
        state.project_cache.borrow().projects.len(),
        2,
        "keep only cached data after dismissing projects"
    );
    state.mode.set(Some(PanelMode::Switch));
    delegate.filter();
    assert!(state.project_matches.borrow().is_empty());
    assert!(state.command_matches.borrow().is_empty());
    assert_eq!(
        NSApplication::sharedApplication(mtm).windows().len(),
        windows_before
    );
    assert!(
        delegate
            .panels()
            .iter()
            .all(|panel| !panel.panel.isVisible())
    );
    println!(
        "Projects: explicit entry, MRU order, project/path search, background selection, scope isolation, missing-path errors and Escape."
    );
}

fn verify_project_open_cancellation(mtm: MainThreadMarker) {
    use crate::macos::platform::project_open::{OpenedProject, tests::pending};
    use std::sync::atomic::Ordering;
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.demo.set(true);
    let origin = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map_or(0, |app| app.processIdentifier());
    let (job, _tx, cancelled) = pending(origin);
    state.project_open.replace(Some(job));
    state.check_panel_focus.set(true);
    delegate.poll(sel!(poll:), None);
    assert!(
        state.project_open.borrow().is_some(),
        "a delayed panel resign event must not cancel a project opening after the picker closed"
    );
    assert!(!cancelled.load(Ordering::Relaxed));
    state.project_open.take();
    let (job, tx, cancelled) = pending(10);
    state.project_open.replace(Some(job));
    delegate.cancel_project_open_if_switched(10, false);
    assert!(state.project_open.borrow().is_some());
    delegate.cancel_project_open_if_switched(20, true);
    assert!(state.project_open.borrow().is_some());
    delegate.cancel_project_open_if_switched(10, false);
    assert!(state.project_open.borrow().is_none());
    assert!(cancelled.load(Ordering::Relaxed));
    assert!(
        tx.send(Ok(OpenedProject {
            pid: -1,
            window: Some(42)
        }))
        .is_err()
    );

    let (job, _tx, cancelled) = pending(10);
    state.project_open.replace(Some(job));
    delegate.end_session();
    assert!(cancelled.load(Ordering::Relaxed));

    let (job, tx, cancelled) = pending(10);
    state.project_open.replace(Some(job));
    tx.send(Ok(OpenedProject {
        pid: -1,
        window: Some(42),
    }))
    .unwrap();
    state.mode.set(Some(PanelMode::Search));
    delegate.poll_project_open();
    assert!(state.project_open.borrow().is_none());
    assert!(cancelled.load(Ordering::Relaxed));
    assert_eq!(state.mode.get(), Some(PanelMode::Search));
    assert!(state.recency.borrow().is_empty());
    assert!(delegate.panels().is_empty());
}
