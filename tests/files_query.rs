use std::path::Path;
use winlane::features::files::{self, Entry, query::Matcher};

fn entry(name: &str, directory: bool) -> Entry {
    Entry {
        path: Path::new("/example").join(name),
        name: name.into(),
        directory,
        modified: 0,
    }
}
fn matcher(pattern: &str) -> Matcher {
    Matcher::new(pattern, Path::new("/example"))
}

#[test]
fn wildcard_queries_need_no_mode_and_keep_literal_matches_first() {
    for (query, yes, no) in [
        ("*.pdf", "Report.PDF", "report.pdf.bak"),
        ("report-?.pdf", "report-1.pdf", "report-12.pdf"),
        ("report*.pdf", "report 2026.pdf", "other.pdf"),
        ("*.tar.gz", "source.tar.gz", "sourceXtarXgz"),
    ] {
        let matcher = matcher(query);
        assert!(matcher.error.is_none());
        assert!(matcher.score(&entry(yes, false)).is_some(), "{query}");
        assert!(matcher.score(&entry(no, false)).is_none(), "{query}");
    }
    let pattern = matcher("report[12].pdf");
    let exact = entry("report[12].pdf", false);
    let mut entries: Vec<_> = (0..150)
        .map(|i| Entry {
            path: Path::new("/example")
                .join(i.to_string())
                .join("report1.pdf"),
            ..entry("report1.pdf", false)
        })
        .collect();
    let recent = entries[..10].to_vec();
    entries.push(exact.clone());
    let results = pattern.matching(&entries, &recent);
    assert_eq!(results.len(), files::MAX_RESULTS);
    assert_eq!(
        results[0], exact,
        "literal names outrank pattern matches and recency"
    );
    assert!(pattern.score(&entry("report2.pdf", false)).is_some());
    assert!(
        !matcher("report.pdf").is_pattern(),
        "a file extension is not a regex switch"
    );
    assert!(matcher("dwn").score(&entry("Downloads", true)).is_some());
}

#[test]
fn common_regular_expressions_match_names_without_matching_parent_paths() {
    for (pattern, yes, no) in [
        (r"^report.*\.pdf$", "Report 2026.PDF", "report.pdf.bak"),
        (r"\.(png|jpe?g)$", "photo.jpeg", "photo.svg"),
        (r"^report-\d{4}\.txt$", "report-2026.txt", "report-new.txt"),
        (r"^会议[一二三]+", "会议一二.md", "笔记.md"),
        (r"(?-i)^README$", "README", "readme"),
        (r"^[^/]+\.rs$", "main.rs", "main.txt"),
    ] {
        let matcher = matcher(pattern);
        assert!(matcher.error.is_none(), "{pattern}");
        assert!(matcher.score(&entry(yes, false)).is_some(), "{pattern}");
        assert!(matcher.score(&entry(no, false)).is_none(), "{pattern}");
    }
    assert!(
        matcher("^example")
            .score(&entry("file.txt", false))
            .is_none()
    );
    assert!(
        matcher("^Doc.*$")
            .score(&entry("Documents", true))
            .is_some()
    );
    for pattern in ["[", "(", "(?<=name)file", r"(a)\1"] {
        let matcher = matcher(pattern);
        assert!(matcher.error.is_some());
        assert_eq!(
            matcher.score(&entry(pattern, false)),
            Some(0),
            "invalid regex never blocks literal names"
        );
    }
    assert!(matcher(&format!("^{}", "a".repeat(1025))).error.is_some());
    assert!(matcher("").score(&entry("anything", false)).is_some());
}

#[test]
fn scoped_patterns_recurse_but_plain_browsing_stays_one_level() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Downloads");
    for dir in ["nested/deeper", ".hidden", "Example.app", "node_modules"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    for name in [
        "root.pdf",
        "nested/inside.PDF",
        "nested/deeper/report.pdf",
        "photo.png",
        ".hidden/secret.pdf",
        "Example.app/internal.pdf",
        "node_modules/generated.pdf",
    ] {
        std::fs::write(root.join(name), "fixture").unwrap();
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&root, root.join("nested/loop")).unwrap();
    for query in ["~/Downloads/*.pdf", r"~/Downloads/^[^/]+\.pdf$"] {
        let matcher = Matcher::new(query, temp.path());
        assert_eq!(matcher.directory.as_deref(), Some(root.as_path()));
        let listing = files::browse::search(&matcher, &[], true, || false, |_| {}).unwrap();
        assert!(!listing.limited);
        let mut names: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        names.sort();
        assert_eq!(names, ["inside.PDF", "report.pdf", "root.pdf"]);
        assert!(files::browse_with(&matcher, || true).unwrap().is_empty());
    }
    let listing = files::browse("~/Downloads/", temp.path(), || false).unwrap();
    assert!(
        listing
            .iter()
            .all(|e| e.path.parent() == Some(root.as_path()))
    );
    assert!(
        files::browse("~/Downloads/down", temp.path(), || false)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        files::query::parent_search_path(r"~/Downloads/^[^/]+\.pdf$", temp.path()).as_deref(),
        Some("~/Downloads/")
    );
    assert_eq!(
        files::query::parent_search_path("~/Downloads/*.pdf", temp.path()).as_deref(),
        Some("~/Downloads/")
    );
    assert_eq!(
        files::query::parent_search_path("~/Downloads/", temp.path()).as_deref(),
        Some("~/")
    );
}

#[test]
fn recursive_results_stay_bounded_without_losing_late_exact_matches() {
    let temp = tempfile::tempdir().unwrap();
    for i in 0..100 {
        let dir = temp.path().join(i.to_string());
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("report1.pdf"), "fixture").unwrap();
    }
    let exact = temp.path().join("report[12].pdf");
    std::fs::write(&exact, "fixture").unwrap();
    let matcher = Matcher::new("~/report[12].pdf", temp.path());
    let listing = files::browse::search(&matcher, &[], true, || false, |_| {}).unwrap();
    assert_eq!(listing.entries.len(), files::MAX_RESULTS);
    assert_eq!(listing.entries[0].path, exact);
    let checks = std::cell::Cell::new(0);
    let _ = files::browse::search(
        &matcher,
        &[],
        true,
        || {
            checks.set(checks.get() + 1);
            checks.get() > 8
        },
        |_| {},
    )
    .unwrap();
    assert!(
        checks.get() < 15,
        "cancellation must stop recursive traversal promptly"
    );
}

#[test]
fn spotlight_literals_never_exclude_pattern_only_matches() {
    let patterns = [
        r"^report.*\.pdf$",
        r"\.(pdf|md)$",
        r"(foo|bar)?report",
        ".*",
        "*.pdf",
        "a*",
        r"\d+",
        "(?i)report",
        "^会议.*笔记$",
        r"file\[\d+\]",
    ];
    let names = [
        "report.pdf",
        "REPORT.PDF",
        "notes.md",
        "barreport",
        "report",
        "123",
        "a",
        "",
        "会议一笔记",
        "file[12]",
    ];
    for pattern in patterns {
        let matcher = matcher(pattern);
        if let Some(literals) = matcher.literals() {
            assert!(!literals.is_empty());
            for name in names {
                if matcher.score(&entry(name, false)) == Some(5) {
                    assert!(
                        literals
                            .iter()
                            .any(|l| name.to_lowercase().contains(&l.to_lowercase())),
                        "{pattern}: {name}, {literals:?}"
                    );
                }
            }
        }
    }
}
