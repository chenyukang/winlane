use std::path::Path;
use winlane::features::files::{
    self, Entry,
    query::{Kind, Matcher, Options},
};

fn entry(name: &str, directory: bool) -> Entry {
    Entry {
        path: Path::new("/example").join(name),
        name: name.into(),
        directory,
        modified: 0,
    }
}
fn regex(pattern: &str) -> Result<Matcher, String> {
    Matcher::new(
        pattern,
        Path::new("/example"),
        Options {
            regex: true,
            kind: Kind::All,
        },
    )
}

#[test]
fn kinds_filter_before_result_limit_and_distinguish_folders_from_extensions() {
    let mut entries: Vec<_> = (0..100)
        .map(|i| entry(&format!("image-{i}.png"), false))
        .collect();
    let folder = entry("folder.pdf", true);
    let document = entry("REPORT.PDF", false);
    entries.extend([folder.clone(), document.clone()]);
    for (kind, expected) in [(Kind::Folders, folder), (Kind::Documents, document)] {
        let matcher =
            Matcher::new("", Path::new("/example"), Options { kind, regex: false }).unwrap();
        assert_eq!(matcher.matching(&entries, &[]), [expected]);
    }
    for (kind, name) in [
        (Kind::Images, "photo.HEIC"),
        (Kind::Audio, "song.flac"),
        (Kind::Video, "movie.mov"),
        (Kind::Archives, "source.tar.gz"),
    ] {
        assert!(kind.allows(&entry(name, false)));
        assert!(!kind.allows(&entry(name, true)));
    }
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
        let matcher = regex(pattern).unwrap();
        assert!(matcher.score(&entry(yes, false)).is_some(), "{pattern}");
        assert!(matcher.score(&entry(no, false)).is_none(), "{pattern}");
    }
    assert!(
        regex("example")
            .unwrap()
            .score(&entry("file.txt", false))
            .is_none()
    );
    assert!(
        regex("^Doc.*$")
            .unwrap()
            .score(&entry("Documents", true))
            .is_some()
    );
    for pattern in ["[", "(", "*", "(?<=name)file", r"(a)\1"] {
        assert!(regex(pattern).is_err());
    }
    assert!(regex(&"a".repeat(1025)).is_err());
    assert!(
        regex("")
            .unwrap()
            .score(&entry("anything", false))
            .is_some()
    );
}

#[test]
fn regex_path_browsing_does_not_treat_regex_punctuation_as_directories() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("Downloads")).unwrap();
    for name in ["report-2026.pdf", "photo.png", "report.txt"] {
        std::fs::write(temp.path().join("Downloads").join(name), "fixture").unwrap();
    }
    for (pattern, count) in [
        (r"~/Downloads/^[^/]+\.pdf$", 1),
        ("~/Downloads/.", 3),
        ("~/Downloads/", 3),
    ] {
        let matcher = Matcher::new(
            pattern,
            temp.path(),
            Options {
                kind: Kind::All,
                regex: true,
            },
        )
        .unwrap();
        assert_eq!(
            matcher.directory.as_deref(),
            Some(temp.path().join("Downloads").as_path())
        );
        assert_eq!(files::browse_with(&matcher, || false).unwrap().len(), count);
        assert!(files::browse_with(&matcher, || true).unwrap().is_empty());
    }
    let plain = Matcher::new("~/Downloads/down", temp.path(), Options::default()).unwrap();
    assert!(
        files::browse_with(&plain, || false).unwrap().is_empty(),
        "the parent folder must not match every child in path browsing"
    );
    assert_eq!(
        files::query::parent_search_path(r"~/Downloads/^[^/]+\.pdf$", temp.path(), true).as_deref(),
        Some("~/Downloads/")
    );
}

#[test]
fn spotlight_literals_never_exclude_regular_expression_matches() {
    let patterns = [
        r"^report.*\.pdf$",
        r"\.(pdf|md)$",
        r"(foo|bar)?report",
        ".*",
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
        let matcher = regex(pattern).unwrap();
        if let Some(literals) = matcher.literals() {
            assert!(!literals.is_empty());
            for name in names {
                if matcher.score(&entry(name, false)).is_some() {
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
