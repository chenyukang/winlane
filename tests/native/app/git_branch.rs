use super::*;

pub(super) fn verify_git_branch_search(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.catalog_checked.set(Some(Instant::now()));
    state.mode.set(Some(PanelMode::Search));
    state.search_scope.set(Some(SearchScope::GitBranch));
    state
        .git_branch_results
        .replace(winlane::features::git_branch::Branches {
            items: vec![
                winlane::features::git_branch::Branch {
                    name: "main".into(),
                    alias: "m".into(),
                    detail: "2 hours ago".into(),
                    current: true,
                    elsewhere: false,
                },
                winlane::features::git_branch::Branch {
                    name: "feature/search".into(),
                    alias: "fs".into(),
                    detail: "1 day ago".into(),
                    current: false,
                    elsewhere: true,
                },
            ],
            error: None,
            repo: Some(std::env::temp_dir().join("winlane-branch-fixture")),
        });
    delegate.filter_preserving(None);
    assert_eq!(delegate.match_count(), 2);
    assert_eq!(delegate.selected_git_branch().unwrap().name, "main");
    for ui in delegate.panels() {
        let rows = ui.rows.borrow();
        assert_eq!(rows[0].app.stringValue().to_string(), "2 hours ago");
        assert!(rows[0].title.stringValue().to_string().contains("main"));
        assert_eq!(rows[0].alias.stringValue().to_string(), "m*");
        assert!(rows[0].icon.image().is_some());
        assert!(!ui.panel.isVisible());
    }
    state.query.replace("feature".into());
    delegate.filter_preserving(None);
    assert_eq!(delegate.match_count(), 1);
    assert_eq!(
        delegate.selected_git_branch().unwrap().name,
        "feature/search"
    );

    delegate.end_session();
    state.mode.set(Some(PanelMode::Search));
    state.search_scope.set(Some(SearchScope::GitBranch));
    state.query.replace(String::new());
    delegate.filter_preserving(None);
    assert_eq!(delegate.match_count(), 2);
    assert_eq!(delegate.selected_git_branch().unwrap().name, "main");
}
