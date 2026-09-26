use rusqlite::Connection;
use std::fs;
use std::path::Path;
use winlane::core::commands::{CommandId, matching_commands};
use winlane::features::open_url::{MAX_RESULTS, MAX_URLS, Page, is_web_url, load, matching};

fn page(url: &str, title: &str, time: i64) -> Page {
    Page {
        url: url.into(),
        title: title.into(),
        last_visit_time: time,
    }
}

#[test]
fn domain_matches_rank_before_title_and_path_matches() {
    let pages = vec![
        page("https://example.test/article", "Video guide", 100),
        page("https://example.test/video", "Guide", 90),
        page("https://myvideo.example.test/", "Guide", 80),
        page("https://videos.example.test/older", "Guide", 5),
        page("https://www.videos.example.test/newer", "Guide", 10),
        page("https://www.video/", "Guide", 1),
    ];
    let results = matching(&pages, " ViDeO ");
    assert_eq!(
        results,
        [5, 4, 3, 2, 0, 1].map(|index| pages[index].clone()),
    );
    assert_eq!(matching(&pages, "videos.example.test").len(), 2);
    assert!(matching(&pages, "missing").is_empty());
}

#[test]
fn domain_ranking_uses_the_host_not_credentials_paths_or_query_parameters() {
    let pages = vec![
        page(
            "https://example.test/?next=https://video.example.test",
            "Guide",
            100,
        ),
        page("https://example.test/video.example.test", "Guide", 90),
        page("https://video.example.test@example.test/", "Guide", 80),
        page("https://video.example.test.other.test/", "Guide", 70),
        page("https://www.video.example.test:8443/watch", "Guide", 1),
    ];
    assert_eq!(
        matching(&pages, "VIDEO.EXAMPLE.TEST"),
        [4, 3, 0, 1, 2].map(|index| pages[index].clone()),
    );
}

#[test]
fn multiple_terms_can_match_both_domain_and_title_and_empty_search_is_recent_first() {
    let pages = vec![
        page("https://example.test/", "Video Rust 中文", 50),
        page("https://video.example.test/newer", "Rust 中文", 20),
        page("https://video.example.test/older", "Rust 中文", 10),
        page("https://video.example.test/other", "Other", 100),
    ];
    assert_eq!(
        matching(&pages, "video RUST 中文"),
        pages[1..3]
            .iter()
            .chain(&pages[..1])
            .cloned()
            .collect::<Vec<_>>()
    );
    assert_eq!(
        matching(&pages, " \t "),
        [3, 0, 1, 2].map(|index| pages[index].clone()),
    );
}

#[test]
fn ranking_happens_before_the_visible_result_limit() {
    let mut pages: Vec<_> = (0..MAX_RESULTS)
        .map(|index| page(&format!("https://example.test/{index}"), "Video guide", 100))
        .collect();
    let domain = page("https://video.example.test/", "Home", 1);
    pages.push(domain.clone());
    let results = matching(&pages, "video");
    assert_eq!(results.len(), MAX_RESULTS);
    assert_eq!(results[0], domain);
}

fn database(root: &Path, profile: &str, wal: bool) -> Connection {
    let path = root.join(profile);
    fs::create_dir_all(&path).unwrap();
    let db = Connection::open(path.join("History")).unwrap();
    if wal {
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
            .unwrap();
    }
    db.execute_batch("CREATE TABLE urls(id INTEGER PRIMARY KEY, url TEXT, title TEXT, last_visit_time INTEGER, hidden INTEGER DEFAULT 0);").unwrap();
    db
}

fn add(db: &Connection, url: &str, title: &str, time: i64) {
    db.execute(
        "INSERT INTO urls(url,title,last_visit_time) VALUES(?1,?2,?3)",
        (url, title, time),
    )
    .unwrap();
}

fn files(root: &Path) -> Vec<(String, Vec<u8>)> {
    ["History", "History-wal", "History-journal", "History-shm"]
        .into_iter()
        .filter_map(|name| {
            fs::read(root.join(name))
                .ok()
                .map(|bytes| (name.into(), bytes))
        })
        .collect()
}

#[test]
fn command_opens_only_the_explicit_history_scope() {
    for query in [
        "open-url",
        "open url",
        "OPEN URL",
        "history",
        "浏览记录",
        "最近网址",
    ] {
        assert_eq!(matching_commands(query), [CommandId::OpenUrl], "{query}");
    }
    for query in [
        "",
        "r",
        "example.com",
        "recent-url",
        "recent urls",
        "recent url",
        "open-url extra",
    ] {
        assert!(!matching_commands(query).contains(&CommandId::OpenUrl));
    }
}

#[test]
fn typed_addresses_open_directly_and_other_text_searches_google() {
    use winlane::features::open_url::{InputTarget, input_target};
    for (input, expected) in [
        (
            " https://example.com/a?q=1&x=2#part ",
            "https://example.com/a?q=1&x=2#part",
        ),
        ("HTTP://EXAMPLE.COM", "http://example.com/"),
        ("example.com/path", "https://example.com/path"),
        ("www.example.com", "https://www.example.com/"),
        ("example.test:8080/a", "https://example.test:8080/a"),
        ("localhost:3000", "http://localhost:3000/"),
        ("127.0.0.1:8080", "http://127.0.0.1:8080/"),
        ("[::1]:8080/a", "http://[::1]:8080/a"),
        (
            "https://example.com/中文",
            "https://example.com/%E4%B8%AD%E6%96%87",
        ),
        ("https://例子.test", "https://xn--fsqu00a.test/"),
    ] {
        assert_eq!(
            input_target(input),
            Some(InputTarget::Url(expected.into())),
            "{input}"
        );
    }
    for input in [
        "rust async",
        "中文搜索",
        "C++ & Rust #traits",
        "2026",
        "rust",
        "user@example.com",
        "example.com invalid",
        "https://",
        "https://?query",
        "https://example.com:99999",
        "https://bad..test",
        "javascript:alert(1)",
        "file:///tmp/test",
        "ftp://example.com",
        "data:text/plain,test",
        "https://example.com\\bad",
        "https://example.com\npath",
    ] {
        let Some(InputTarget::Search(target)) = input_target(input) else {
            panic!("expected Google search for {input:?}");
        };
        let url = url::Url::parse(&target).unwrap();
        assert_eq!(url.host_str(), Some("www.google.com"));
        assert_eq!(url.path(), "/search");
        assert_eq!(url.fragment(), None);
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            [("q".into(), input.into())]
        );
    }
    for input in ["", " ", "\n\t"] {
        assert!(input_target(input).is_none());
    }
}

#[test]
fn profiles_merge_by_recency_deduplicate_and_search_title_or_url() {
    let root = tempfile::tempdir().unwrap();
    let first = database(root.path(), "Default", false);
    let second = database(root.path(), "Profile 2", false);
    let guest = database(root.path(), "Guest Profile", false);
    add(&first, "https://example.com/rust", "Old Rust title", 10);
    add(&first, "https://example.com/older", "文档", 5);
    add(&first, "https://example.com/hidden", "Hidden", 500);
    first
        .execute("UPDATE urls SET hidden=1 WHERE url LIKE '%hidden'", [])
        .unwrap();
    add(&first, "https://example.com/unvisited", "Not visited", 0);
    for url in [
        "chrome://settings",
        "file:///tmp/private",
        "javascript:alert(1)",
        "https:///bad",
        "https://example.com/\n",
    ] {
        add(&first, url, "Invalid", 999);
    }
    add(&second, "https://example.com/rust", "Rust 新标题", 30);
    add(&second, "https://example.test/blank", "", 20);
    add(&guest, "https://example.com/guest", "Guest", 1000);
    let history = load(root.path());
    assert!(history.error.is_none(), "{:?}", history.error);
    assert_eq!(history.pages.len(), 3);
    assert_eq!(history.pages[0].title, "Rust 新标题");
    assert_eq!(history.pages[1].title, "https://example.test/blank");
    assert_eq!(matching(&history.pages, "RUST 新"), history.pages[..1]);
    assert_eq!(matching(&history.pages, "example.com").len(), 2);
    assert_eq!(matching(&history.pages, "文档")[0].last_visit_time, 5);
    assert!(matching(&history.pages, "absent").is_empty());
}

#[test]
fn exclusive_lock_and_uncommitted_rollback_journal_preserve_source_and_committed_history() {
    let root = tempfile::tempdir().unwrap();
    let db = database(root.path(), "Default", false);
    add(&db, "https://example.com/committed", "Committed", 10);
    db.execute_batch("PRAGMA locking_mode=EXCLUSIVE; PRAGMA cache_size=1; BEGIN EXCLUSIVE;")
        .unwrap();
    db.execute("UPDATE urls SET title=?1", ["uncommitted".repeat(10000)])
        .unwrap();
    // Force dirty pages into the main file, requiring journal recovery on the copy.
    for i in 0..10 {
        add(
            &db,
            &format!("https://example.com/uncommitted/{i}"),
            &"draft".repeat(10000),
            20,
        );
    }
    let profile = root.path().join("Default");
    let before = files(&profile);
    let history = load(root.path());
    assert!(history.error.is_none(), "{:?}", history.error);
    assert_eq!(history.pages.len(), 1);
    assert_eq!(history.pages[0].title, "Committed");
    assert_eq!(
        before,
        files(&profile),
        "source database and journals must stay untouched"
    );
    db.execute_batch("ROLLBACK;").unwrap();
}

#[test]
fn wal_history_includes_committed_writes_but_never_uncommitted_ones() {
    let root = tempfile::tempdir().unwrap();
    let db = database(root.path(), "Default", true);
    add(&db, "https://example.com/new", "From WAL", 50);
    db.execute_batch("BEGIN IMMEDIATE;").unwrap();
    add(&db, "https://example.com/draft", "Uncommitted", 100);
    let profile = root.path().join("Default");
    let before = files(&profile);
    let history = load(root.path());
    assert!(history.error.is_none(), "{:?}", history.error);
    assert_eq!(history.pages.len(), 1);
    assert_eq!(history.pages[0].title, "From WAL");
    assert_eq!(before, files(&profile));
    db.execute_batch("ROLLBACK; DELETE FROM urls;").unwrap();
    assert!(
        load(root.path()).pages.is_empty(),
        "refresh must reflect deleted browser history"
    );
}

#[test]
fn results_are_bounded_and_partial_profile_errors_are_visible() {
    let root = tempfile::tempdir().unwrap();
    let mut db = database(root.path(), "Default", false);
    let tx = db.transaction().unwrap();
    for i in 0..MAX_URLS + 10 {
        add(
            &tx,
            &format!("https://example.com/{i}"),
            "Page",
            (i + 1) as i64,
        );
    }
    tx.commit().unwrap();
    fs::create_dir(root.path().join("Profile 1")).unwrap();
    fs::write(
        root.path().join("Profile 1/History"),
        b"not a SQLite database",
    )
    .unwrap();
    let history = load(root.path());
    assert_eq!(history.pages.len(), MAX_URLS);
    assert_eq!(
        matching(&history.pages, "").len(),
        winlane::features::open_url::MAX_RESULTS
    );
    assert_eq!(history.pages[0].last_visit_time, (MAX_URLS + 10) as i64);
    assert!(history.error.is_some());
    assert!(
        !history.access_denied,
        "a corrupt database is not a permission failure"
    );
    assert!(load(&root.path().join("missing")).pages.is_empty());
    assert!(load(&root.path().join("missing")).error.is_none());
    for url in [
        "javascript:alert(1)",
        "file:///tmp/test",
        "https://",
        "https://?x",
        "https://example.com/\n",
    ] {
        assert!(!is_web_url(url));
    }
}

#[cfg(unix)]
#[test]
fn permission_denials_are_actionable_and_recover_after_access_is_restored() {
    use std::os::unix::fs::PermissionsExt;

    struct Restricted(std::path::PathBuf, fs::Permissions);
    impl Restricted {
        fn new(path: &Path) -> Self {
            let permissions = fs::metadata(path).unwrap().permissions();
            fs::set_permissions(path, fs::Permissions::from_mode(0o000)).unwrap();
            Self(path.into(), permissions)
        }
    }
    impl Drop for Restricted {
        fn drop(&mut self) {
            fs::set_permissions(&self.0, self.1.clone()).unwrap();
        }
    }

    let root = tempfile::tempdir().unwrap();
    let db = database(root.path(), "Default", false);
    add(&db, "https://example.test/private", "History", 1);
    drop(db);
    for path in [
        root.path().to_owned(),
        root.path().join("Default"),
        root.path().join("Default/History"),
    ] {
        let restriction = Restricted::new(&path);
        let history = load(root.path());
        assert!(history.pages.is_empty());
        assert!(
            history.access_denied,
            "permission failure was swallowed: {:?}",
            history.error
        );
        assert!(history.error.is_some());
        drop(restriction);
        let history = load(root.path());
        assert!(!history.access_denied);
        assert!(history.error.is_none());
        assert_eq!(history.pages.len(), 1);
    }
    let other = database(root.path(), "Profile 1", false);
    add(&other, "https://example.com/available", "Available", 2);
    drop(other);
    let _restriction = Restricted::new(&root.path().join("Default/History"));
    let history = load(root.path());
    assert!(history.access_denied);
    assert_eq!(history.pages.len(), 1, "readable profiles still appear");
    assert_eq!(history.pages[0].title, "Available");
}

#[test]
#[ignore = "Reads local Chrome history only when explicitly requested; prints counts, never URLs"]
fn local_chrome_history_read() {
    let home = std::env::var_os("HOME").unwrap();
    let start = std::time::Instant::now();
    let history = load(&winlane::features::open_url::chrome_directory(Path::new(
        &home,
    )));
    assert!(history.error.is_none(), "{:?}", history.error);
    assert!(!history.pages.is_empty());
    println!(
        "Chrome history: {} URLs loaded in {:?}; no pages opened.",
        history.pages.len(),
        start.elapsed()
    );
}

#[test]
fn escape_like_escapes_sql_wildcards() {
    use winlane::features::open_url::escape_like;
    assert_eq!(escape_like("plain"), "plain");
    assert_eq!(escape_like("a_b"), "a\\_b");
    assert_eq!(escape_like("100%"), "100\\%");
    assert_eq!(escape_like("a\\b"), "a\\\\b");
}

#[test]
fn deep_search_reads_past_the_recent_window_in_the_default_profile() {
    let root = tempfile::tempdir().unwrap();
    let mut db = database(root.path(), "Default", false);
    let tx = db.transaction().unwrap();
    // One old page the bounded recent window cannot hold...
    add(&tx, "https://example.test/old/dotbackup", "Old repo", 1);
    for i in 0..MAX_URLS + 5 {
        add(
            &tx,
            &format!("https://example.test/recent/{i}"),
            "Recent page",
            (100 + i) as i64,
        );
    }
    tx.commit().unwrap();
    // ...and an equally old one in another profile, which must stay invisible.
    let other = database(root.path(), "Profile 1", false);
    add(&other, "https://other.test/old/dotbackup", "Other repo", 2);

    let history = load(root.path());
    assert!(
        matching(&history.pages, "dotbackup").is_empty(),
        "the old URL must be outside the bounded recent window"
    );
    let deep = winlane::features::open_url::deep_search_default(root.path(), "dotbackup").unwrap();
    assert_eq!(
        deep.len(),
        1,
        "only the Default profile is searched: {deep:?}"
    );
    assert_eq!(deep[0].url, "https://example.test/old/dotbackup");
    assert!(
        winlane::features::open_url::deep_search_default(root.path(), "   ")
            .unwrap()
            .is_empty(),
        "an empty query does not search"
    );
    assert!(
        winlane::features::open_url::deep_search_default(root.path(), "absent")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn deep_search_matches_literally_and_skips_hidden_rows() {
    let root = tempfile::tempdir().unwrap();
    let db = database(root.path(), "Default", false);
    add(&db, "https://video.example.test/a_b", "Guide", 5);
    add(&db, "https://video.example.test/axb", "Guide", 4);
    add(&db, "https://example.test/a_b", "Video guide", 3);
    add(&db, "https://example.test/hidden", "Video guide a_b", 9);
    db.execute("UPDATE urls SET hidden=1 WHERE url LIKE '%hidden'", [])
        .unwrap();
    let deep = winlane::features::open_url::deep_search_default(root.path(), "a_b").unwrap();
    let urls: Vec<_> = deep.iter().map(|page| page.url.as_str()).collect();
    assert_eq!(
        urls,
        ["https://video.example.test/a_b", "https://example.test/a_b",],
        "literal `_` is matched, hidden rows are excluded, most recent first"
    );
}
