use super::*;

pub(super) fn verify_scoped_loading(mtm: MainThreadMarker) {
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
    tx.send(winlane::features::projects::Cache::default())
        .unwrap();
    delegate.poll_projects();
    assert!(state.project_receiver.borrow().is_none());
    delegate.end_session();
}
