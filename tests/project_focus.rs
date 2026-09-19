use winlane::features::projects::focus::{ReadyWindow, Window, is_origin_or_target, target_window};
use winlane::features::projects::{Kind, Project};

fn project(path: &str, kind: Kind) -> Project {
    Project {
        path: path.into(),
        name: "Display label".into(),
        kind,
    }
}

fn window(id: u64, title: &str, document: Option<&str>) -> Window {
    Window {
        id,
        title: title.into(),
        document: document.map(str::to_owned),
    }
}

#[test]
fn waits_for_the_requested_project_instead_of_the_first_code_window() {
    let project = project("/example/fiber", Kind::Folder);
    let mut windows = vec![
        window(1, "main.rs — rust", None),
        window(2, "Welcome", None),
    ];
    assert_eq!(target_window(&project, &windows), None);
    windows[1].title = "● README.md — fiber — Visual Studio Code".into();
    assert_eq!(target_window(&project, &windows), Some(2));
    windows[1].title = "README.md - fiber - Visual Studio Code".into();
    assert_eq!(target_window(&project, &windows), Some(2));
    windows[1].title = "fiber-tools — rust".into();
    assert_eq!(target_window(&project, &windows), None);
}

#[test]
fn exact_document_paths_disambiguate_same_named_projects_without_substring_matches() {
    let project = project("/example/one/fiber", Kind::Folder);
    let mut windows = vec![window(1, "fiber", None), window(2, "fiber", None)];
    assert_eq!(target_window(&project, &windows), None);
    windows[0].document = Some("file:///example/one/fiber-other/src/lib.rs".into());
    windows[1].document = Some("file:///example/one/fiber/src/lib.rs".into());
    assert_eq!(target_window(&project, &windows), Some(2));
    windows[0].document = windows[1].document.clone();
    assert_eq!(target_window(&project, &windows), None);
    assert_eq!(
        target_window(
            &project,
            &[window(
                3,
                "fiber",
                Some("file:///example/two/fiber/src/main.rs")
            )]
        ),
        None
    );
}

#[test]
fn window_readiness_requires_two_consecutive_observations() {
    let mut ready = ReadyWindow::default();
    assert_eq!(ready.observe(None), None);
    assert_eq!(ready.observe(Some(10)), None);
    assert_eq!(ready.observe(None), None);
    assert_eq!(ready.observe(Some(10)), None);
    assert_eq!(ready.observe(Some(20)), None);
    assert_eq!(ready.observe(Some(20)), Some(20));
}

#[test]
fn workspaces_unicode_and_project_names_with_separators_are_supported() {
    let workspace = project("/example/My Workspace.code-workspace", Kind::Workspace);
    assert_eq!(
        target_window(
            &workspace,
            &[window(
                1,
                "main.rs — My Workspace (Workspace) — Visual Studio Code",
                None
            )]
        ),
        Some(1)
    );
    let folder = project("/example/世界 - project", Kind::Folder);
    assert_eq!(
        target_window(&folder, &[window(2, "main.rs — 世界 - project", None)]),
        Some(2)
    );
    assert_eq!(
        target_window(
            &folder,
            &[window(
                2,
                "Custom title",
                Some("file:///example/%E4%B8%96%E7%95%8C%20-%20project")
            )]
        ),
        Some(2)
    );
    assert_eq!(
        target_window(
            &folder,
            &[window(
                2,
                "Nested workspace",
                Some("file:///example/%E4%B8%96%E7%95%8C%20-%20project/child/src/main.rs")
            )]
        ),
        None
    );
}

#[test]
fn changing_apps_cancels_focus_even_when_returning_to_the_origin_after_code() {
    let mut seen = false;
    assert!(is_origin_or_target(10, 10, false, &mut seen));
    assert!(!is_origin_or_target(10, 20, false, &mut seen));
    assert!(is_origin_or_target(10, 30, true, &mut seen));
    assert!(!is_origin_or_target(10, 10, false, &mut seen));
}
