use std::path::{Path, PathBuf};
use winlane::features::files::{self, Entry, Settings};

fn entry(path: &str, modified: u64) -> Entry {
    Entry {
        path: path.into(),
        name: Path::new(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        directory: false,
        modified,
    }
}

#[test]
fn filename_relevance_precedes_recency_and_path_matches() {
    let exact = entry("/docs/report.pdf", 1);
    let prefix = entry("/docs/report-final.pdf", 2);
    let substring = entry("/docs/annual-report.pdf", 3);
    let path = entry("/docs/report/notes.pdf", 100);
    let fuzzy = entry("/docs/r-e-p-o-r-t.pdf", 200);
    let list = vec![
        fuzzy.clone(),
        path.clone(),
        substring.clone(),
        prefix.clone(),
        exact.clone(),
    ];
    assert_eq!(
        files::matching(&list, "report", &[fuzzy.clone(), path.clone()]),
        vec![exact, prefix, substring, path, fuzzy]
    );
    assert!(files::matching(&list, "missing", &[]).is_empty());
}

#[test]
fn terms_match_filename_and_parent_with_unicode_and_case() {
    let readme = entry("/projects/website/README.md", 1);
    let chinese = entry("/docs/工作/会议记录.txt", 2);
    assert_eq!(
        files::matching(std::slice::from_ref(&readme), "website readme", &[]),
        vec![readme]
    );
    assert_eq!(
        files::matching(std::slice::from_ref(&chinese), "工作 会议", &[]),
        vec![chinese]
    );
    assert!(files::score(&entry("/docs/abc.txt", 0), "az").is_none());
}

#[test]
fn result_limits_deduplication_and_recent_order() {
    let mut entries: Vec<_> = (0..80)
        .map(|i| entry(&format!("/docs/item-{i}.txt"), i))
        .collect();
    let recent = vec![entries[1].clone(), entries[0].clone()];
    entries.push(entries[0].clone());
    let matches = files::matching(&entries, "item", &recent);
    assert_eq!(matches.len(), files::MAX_RESULTS);
    assert_eq!(&matches[..2], &recent);
    let mut recent = Vec::new();
    for item in &entries {
        files::remember(&mut recent, item.clone());
    }
    assert_eq!(recent.len(), files::MAX_RECENT);
    assert_eq!(recent[0], entries[0]);
}

#[test]
fn search_scope_excludes_complete_paths_and_generated_folders() {
    let home = Path::new("/Users/example");
    let mut settings = Settings::default();
    assert!(settings.allows(&home.join("Documents/note.txt"), home));
    for path in [
        "Library/note.txt",
        ".secret",
        "repo/target/debug/main",
        "App.app/Contents/data",
    ] {
        assert!(!settings.allows(&home.join(path), home));
    }
    assert!(settings.allows(&home.join("Library-notes/note.txt"), home));
    assert!(!settings.allows(Path::new("/Volumes/External/file"), home));
    settings.hide_generated = false;
    assert!(settings.allows(&home.join("repo/target/main"), home));
    settings.roots = vec!["relative".into()];
    assert!(settings.validate().is_err());
    settings.roots.clear();
    assert!(settings.validate().is_err());
    let config = winlane::core::config::Config::default();
    let restored: winlane::core::config::Config = serde_json::from_str("{}").unwrap();
    assert_eq!(restored.files, config.files);
}

#[test]
fn paths_and_directory_browsing_do_not_need_spotlight() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    std::fs::create_dir(home.join("Documents")).unwrap();
    std::fs::write(home.join("Documents/Report 2026.txt"), "sample").unwrap();
    std::fs::write(home.join("Documents/.hidden"), "sample").unwrap();
    assert_eq!(
        files::path_query("~/Documents/", home),
        Some((home.join("Documents"), String::new()))
    );
    assert_eq!(
        files::expand("~/Documents/../Documents", home),
        Some(home.join("Documents"))
    );
    assert_eq!(files::expand("/../../", home), Some(PathBuf::from("/")));
    assert!(files::path_query("report", home).is_none());
    let results = files::browse("~/Documents/", home, || false).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Report 2026.txt");
    assert_eq!(results[0].parent_label(home), "~/Documents");
    assert_eq!(
        files::browse("~/Documents/.h", home, || false)
            .unwrap()
            .len(),
        1
    );
    assert!(
        files::browse("~/Documents/", home, || true)
            .unwrap()
            .is_empty()
    );
    assert!(files::browse("~/missing/", home, || false).is_err());
}

#[test]
fn folder_abbreviations_work_in_name_search_and_path_browsing() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    for name in ["Documents", "Downloads", "DC", "工作资料"] {
        std::fs::create_dir(home.join(name)).unwrap();
    }
    std::fs::write(home.join("Documents.txt"), "fixture").unwrap();
    let entries = files::browse("~/", home, || false).unwrap();
    let names = |query| {
        let named: Vec<_> = entries
            .iter()
            .cloned()
            .map(|mut entry| {
                entry.path = Path::new("/fixtures").join(&entry.name);
                entry
            })
            .collect();
        files::matching(&named, query, &[])
            .into_iter()
            .map(|e| e.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(names("dc"), ["DC", "Documents"]);
    assert_eq!(names("dcm"), ["Documents", "Documents.txt"]);
    assert_eq!(names("dwn"), ["Downloads"]);
    assert_eq!(names("工资"), ["工作资料"]);
    assert!(names("dx").is_empty());
    let matches = files::browse("~/dc", home, || false).unwrap();
    assert_eq!(
        files::matching(&matches, "dc", &[])
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>(),
        ["DC", "Documents"]
    );
}

#[test]
fn completion_enters_folders_and_preserves_file_paths() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let folder = home.join("工作 📂");
    std::fs::create_dir(&folder).unwrap();
    let path = folder.join("report 2026.txt");
    std::fs::write(&path, "fixture").unwrap();
    let directory = Entry::read(folder).unwrap();
    let file = Entry::read(path.clone()).unwrap();
    assert_eq!(directory.completion(home), "~/工作 📂/");
    assert_eq!(file.completion(home), "~/工作 📂/report 2026.txt");
    assert_eq!(
        files::browse(&directory.completion(home), home, || false).unwrap(),
        std::slice::from_ref(&file)
    );
    assert_eq!(files::expand(&file.completion(home), home), Some(path));
    assert_eq!(Entry::read(home.into()).unwrap().completion(home), "~/");
    assert_eq!(Entry::read("/".into()).unwrap().completion(home), "/");
    assert_eq!(
        file.completion(Path::new("/other-home")),
        file.path.to_string_lossy()
    );
}

#[test]
fn parent_navigation_removes_one_component_and_stops_at_roots() {
    for (query, expected) in [
        ("~/Downloads/", "~/"),
        ("~/Downloads/reports/", "~/Downloads/"),
        ("~/Downloads/report", "~/Downloads/"),
        ("~/工作 📂/会议记录/", "~/工作 📂/"),
        ("~/Downloads//reports///", "~/Downloads/"),
        ("~/", "~/"),
        ("~", "~/"),
        ("/Volumes/Archive/", "/Volumes/"),
        ("/Volumes/", "/"),
        ("/", "/"),
    ] {
        assert_eq!(
            files::parent_query(query).as_deref(),
            Some(expected),
            "{query}"
        );
    }
    for query in ["", "report", "project readme", "relative/path", "~someone/"] {
        assert!(files::parent_query(query).is_none());
    }
}

#[test]
fn history_roundtrips_atomically_prunes_missing_files_and_clears() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("sample.txt");
    std::fs::write(&file, "sample").unwrap();
    let saved = Entry::read(file.clone()).unwrap();
    let path = files::history::path(temp.path());
    files::history::save(&path, &[saved.clone(), saved.clone()]).unwrap();
    assert_eq!(files::history::load(&path).unwrap(), vec![saved]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    std::fs::remove_file(&file).unwrap();
    assert!(files::history::load(&path).unwrap().is_empty());
    files::history::save(&path, &[]).unwrap();
    assert!(files::history::load(&path).unwrap().is_empty());
    std::fs::write(&path, "not json").unwrap();
    assert!(files::history::load(&path).is_err());
}
