use std::fs;
use winlane::features::git_branch::{Branch, build_branches, matching};

fn branch(name: &str, detail: &str, current: bool, elsewhere: bool) -> Branch {
    Branch {
        name: name.into(),
        alias: winlane::features::git_branch::alias(name),
        detail: detail.into(),
        current,
        elsewhere,
    }
}

#[test]
fn matching_filters_by_branch_name() {
    let items = vec![
        branch("main", "2 hours ago", true, false),
        branch("feature/search", "1 day ago", false, false),
        branch("bugfix/ui", "3 days ago", false, true),
    ];
    let names = |query: &str| {
        matching(&items, query)
            .into_iter()
            .map(|item| item.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(names(""), ["main", "feature/search", "bugfix/ui"]);
    assert_eq!(names("feature"), ["feature/search"]);
    assert_eq!(names("UI"), ["bugfix/ui"]);
    assert!(names("zzz").is_empty());
}

#[test]
fn matching_filters_by_generated_alias() {
    let items = vec![
        branch(
            "zhangsoledad/txpool-true-shard-authority",
            "8 days ago",
            false,
            false,
        ),
        branch("treasury-web-demo", "5 weeks ago", false, false),
    ];
    let names = |query: &str| {
        matching(&items, query)
            .into_iter()
            .map(|item| item.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(names("zt"), ["zhangsoledad/txpool-true-shard-authority"]);
    assert_eq!(names("tw"), ["treasury-web-demo"]);
}

#[test]
fn alias_uses_branch_name_initials() {
    assert_eq!(winlane::features::git_branch::alias("main"), "m");
    assert_eq!(winlane::features::git_branch::alias("feature/search"), "fs");
    assert_eq!(winlane::features::git_branch::alias("bugfix/ui"), "bu");
}

#[test]
fn build_branches_prefers_recent_checkouts_then_commits() {
    let refs = "main\t1 minute ago\t*\t\nfeature\t2 hours ago\t\t\nbugfix\t3 days ago\t\t/worktrees/bugfix\n";
    let reflog = "checkout: moving from bugfix to feature\ncheckout: moving from feature to main\n";
    let items = build_branches(refs, reflog);
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].name, "feature");
    assert!(!items[0].current);
    assert!(!items[0].elsewhere);
    assert_eq!(items[1].name, "main");
    assert!(items[1].current);
    assert_eq!(items[2].name, "bugfix");
    assert!(items[2].elsewhere);
}

#[test]
fn checkout_switches_branches_in_a_temporary_repository() {
    let root = std::env::temp_dir().join(format!("winlane-branch-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let run = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run(&["init"]);
    run(&["config", "user.name", "Winlane Test"]);
    run(&["config", "user.email", "winlane@example.com"]);
    fs::write(root.join("README.md"), "main\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-m", "init"]);
    run(&["branch", "feature"]);
    let switch = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["switch", "feature"])
        .output()
        .unwrap();
    assert!(
        switch.status.success(),
        "git switch feature: {}",
        String::from_utf8_lossy(&switch.stderr)
    );
    let head = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["branch", "--show-current"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&head.stdout).trim(), "feature");
    let _ = fs::remove_dir_all(&root);
}
