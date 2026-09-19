use rusqlite::Connection;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use winlane::features::projects::{
    Cache, Kind, MAX_PROJECTS, MAX_RESULTS, Project, Sources, matching, snapshot,
};

fn many_projects() -> Vec<Project> {
    (0..MAX_PROJECTS)
        .map(|index| Project {
            path: PathBuf::from(format!("/example/project-{index:04}")),
            name: format!("Project {index:04}"),
            kind: Kind::Folder,
        })
        .collect()
}

#[test]
fn visible_results_are_bounded_but_search_includes_the_entire_history() {
    let projects = many_projects();
    assert_eq!(matching(&projects, ""), projects[..MAX_RESULTS]);
    assert_eq!(matching(&projects, "project"), projects[..MAX_RESULTS]);
    assert_eq!(matching(&projects, "0499"), vec![projects[499].clone()]);
}

#[test]
fn project_snapshot_survives_restart_and_preserves_only_25_recent_projects() {
    let directory = tempfile::tempdir().unwrap();
    let path = snapshot::path(directory.path());
    let mut projects = many_projects();
    projects[1].kind = Kind::Workspace;
    projects[1].name = "Workspace 世界".into();
    projects.insert(2, projects[0].clone());
    let expected: Vec<_> = std::iter::once(projects[0].clone())
        .chain(std::iter::once(projects[1].clone()))
        .chain(projects[3..].iter().cloned())
        .take(MAX_RESULTS)
        .collect();
    snapshot::save(&path, &projects).unwrap();
    assert_eq!(snapshot::load(&path).unwrap(), expected);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let before = fs::read(&path).unwrap();
    projects[0].name = "x".repeat(300 * 1024);
    assert!(snapshot::save(&path, &projects).is_err());
    assert_eq!(
        fs::read(&path).unwrap(),
        before,
        "failed cache writes are atomic"
    );
    snapshot::save(&path, &[]).unwrap();
    assert!(
        snapshot::load(&path).unwrap().is_empty(),
        "an empty successful history clears old cached projects"
    );
}

#[test]
fn malformed_or_oversized_project_snapshots_do_not_prevent_a_fresh_load() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("projects.json");
    assert!(snapshot::load(&path).is_err());
    for invalid in [
        "invalid".to_owned(),
        r#"{"version":2,"projects":[]}"#.to_owned(),
        " ".repeat(300 * 1024),
    ] {
        fs::write(&path, invalid).unwrap();
        assert!(snapshot::load(&path).is_err());
    }
    fs::write(
        &path,
        json!({"version":1,"projects":[
            {"path":"relative","name":"Bad","kind":"Folder"},
            {"path":"/example/valid","name":"Valid","kind":"Folder"}
        ]})
        .to_string(),
    )
    .unwrap();
    assert_eq!(snapshot::load(&path).unwrap().len(), 1);
    snapshot::save(&path, &many_projects()).unwrap();
    assert_eq!(snapshot::load(&path).unwrap().len(), MAX_RESULTS);
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "winlane-projects-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn sources(&self) -> Sources {
        Sources {
            databases: vec![self.0.join("shared.db"), self.0.join("legacy.db")],
            storage: self.0.join("storage.json"),
        }
    }
    fn folder(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(&path).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn uri(path: &Path) -> String {
    const ESCAPE: &percent_encoding::AsciiSet = &percent_encoding::CONTROLS
        .add(b' ')
        .add(b'%')
        .add(b'#')
        .add(b'?')
        .add(b'"');
    format!(
        "file://{}",
        percent_encoding::utf8_percent_encode(&path.to_string_lossy(), ESCAPE)
    )
}

fn database(path: &Path) -> Connection {
    let db = Connection::open(path).unwrap();
    db.execute_batch("CREATE TABLE ItemTable (key TEXT UNIQUE, value TEXT)")
        .unwrap();
    db
}
fn save(db: &Connection, key: &str, entries: Value) {
    db.execute(
        "INSERT OR REPLACE INTO ItemTable VALUES (?1, ?2)",
        (key, json!({"entries":entries}).to_string()),
    )
    .unwrap();
}

#[test]
fn history_order_key_priority_decoding_deduplication_and_read_only() {
    let fixture = Fixture::new();
    let sources = fixture.sources();
    let alpha = fixture.folder("Alpha 世界 # %");
    let beta = fixture.folder("Beta");
    let workspace = fixture.0.join("My Workspace.code-workspace");
    fs::write(&workspace, "{}").unwrap();
    let db = database(&sources.databases[0]);
    save(
        &db,
        "history.recentlyOpenedPathsList",
        json!([{"folderUri":uri(&beta)}]),
    );
    save(
        &db,
        "recently.opened",
        json!([
            {"folderUri":uri(&alpha)}, {"workspace":{"configPath":uri(&workspace)}},
            {"folderUri":uri(&beta)}, {"folderUri":format!("{}/",uri(&alpha))},
            {"fileUri":uri(&fixture.0.join("main.rs"))},
            {"folderUri":"vscode-remote://ssh-remote+example/home/project"},
            {"folderUri":uri(&beta),"remoteAuthority":"ssh-remote+example"},
            {"folderUri":"file://server/share/project"}, {"folderUri":"file:///tmp/%XX"},
            {"folderUri":"file:///tmp/%00bad"}, {"folderUri":"file:///tmp/%FF"}
        ]),
    );
    drop(db);
    let before = fs::read(&sources.databases[0]).unwrap();
    let mut cache = Cache::default();
    assert!(cache.refresh(&sources, false));
    assert!(cache.error.is_none(), "{:?}", cache.error);
    assert_eq!(
        cache
            .projects
            .iter()
            .map(|p| p.path.clone())
            .collect::<Vec<_>>(),
        [alpha, workspace, beta]
    );
    assert_eq!(cache.projects[0].name, "Alpha 世界 # %");
    assert_eq!(cache.projects[1].name, "My Workspace");
    assert_eq!(cache.projects[1].kind, Kind::Workspace);
    assert!(cache.projects.iter().all(|p| p.validate_path().is_ok()));
    assert_eq!(fs::read(&sources.databases[0]).unwrap(), before);
    assert!(
        !cache.refresh(&sources, false),
        "unchanged sources should reuse the cache"
    );
    assert_eq!(matching(&cache.projects, "世界 alpha").len(), 1);
    assert_eq!(
        matching(&cache.projects, "MY workspace")[0].kind,
        Kind::Workspace
    );
    assert_eq!(matching(&cache.projects, ""), cache.projects);
    fs::remove_dir(&cache.projects[2].path).unwrap();
    assert!(
        cache.projects[2].validate_path().is_err(),
        "deleted paths should report an error when opened"
    );
}

#[test]
fn storage_metadata_supplements_history_without_reordering_it() {
    let fixture = Fixture::new();
    let sources = fixture.sources();
    let a = fixture.folder("a");
    let b = fixture.folder("b");
    let c = fixture.folder("c");
    let db = database(&sources.databases[1]);
    save(
        &db,
        "history.recentlyOpenedPathsList",
        json!([{"folderUri":uri(&a)}]),
    );
    fs::write(&sources.storage,json!({"backupWorkspaces":{"folders":[{"folderUri":uri(&a)},{"folderUri":uri(&b)}]},"profileAssociations":{"workspaces":{uri(&c):"profile-id"}}}).to_string()).unwrap();
    let mut cache = Cache::default();
    cache.refresh(&sources, false);
    assert_eq!(
        cache
            .projects
            .iter()
            .map(|p| p.path.clone())
            .collect::<Vec<_>>(),
        [a, b, c]
    );
    let shared = database(&sources.databases[0]);
    save(&shared, "recently.opened", json!([]));
    fs::write(&sources.storage, "{}").unwrap();
    cache.refresh(&sources, false);
    assert!(
        cache.projects.is_empty(),
        "an authoritative empty shared history must not resurrect the old DB"
    );
    assert!(cache.error.is_none());
}

#[test]
fn cache_observes_wal_commits_and_explicit_refresh() {
    let fixture = Fixture::new();
    let sources = fixture.sources();
    let db = database(&sources.databases[0]);
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
        .unwrap();
    let a = fixture.folder("a");
    let b = fixture.folder("b");
    save(&db, "recently.opened", json!([{"folderUri":uri(&a)}]));
    let mut cache = Cache::default();
    assert!(cache.refresh(&sources, false));
    assert!(!cache.refresh(&sources, false));
    let main = fs::read(&sources.databases[0]).unwrap();
    save(
        &db,
        "recently.opened",
        json!([{"folderUri":uri(&b)},{"folderUri":uri(&a)}]),
    );
    assert_eq!(
        fs::read(&sources.databases[0]).unwrap(),
        main,
        "update should still be in the WAL"
    );
    assert!(cache.refresh(&sources, false));
    assert_eq!(cache.projects[0].path, b);
    assert!(!cache.refresh(&sources, false));
    assert!(cache.refresh(&sources, true));
}

#[test]
fn missing_malformed_locked_and_recovered_data() {
    let fixture = Fixture::new();
    let sources = fixture.sources();
    let a = fixture.folder("a");
    let mut cache = Cache::default();
    cache.refresh(&sources, false);
    assert!(cache.error.is_none());
    assert!(cache.projects.is_empty());
    assert!(
        !sources.databases[0].exists(),
        "reading missing history must not create a database"
    );
    let db = database(&sources.databases[0]);
    save(&db, "recently.opened", json!([{"folderUri":uri(&a)}]));
    cache.refresh(&sources, false);
    assert_eq!(cache.projects.len(), 1);
    db.execute_batch("BEGIN EXCLUSIVE;").unwrap();
    assert!(cache.refresh(&sources, true));
    assert!(cache.error.is_some());
    assert_eq!(
        cache.projects.len(),
        1,
        "keep cached projects on a transient read failure"
    );
    db.execute_batch("ROLLBACK;").unwrap();
    cache.refresh(&sources, false);
    assert!(cache.error.is_none());
    db.execute("UPDATE ItemTable SET value='invalid'", [])
        .unwrap();
    cache.refresh(&sources, false);
    assert!(cache.error.is_some());
    assert_eq!(cache.projects.len(), 1);
    save(&db, "recently.opened", json!([]));
    fs::write(&sources.storage, "invalid").unwrap();
    cache.refresh(&sources, false);
    assert!(cache.error.is_some());
    fs::write(&sources.storage, "{}").unwrap();
    cache.refresh(&sources, false);
    assert!(cache.projects.is_empty());
    assert!(cache.error.is_none());
}

#[test]
fn source_discovery_reads_product_metadata_and_restricts_shared_folder_names() {
    let fixture = Fixture::new();
    let app = fixture.folder("Code.app/Contents/Resources/app");
    fs::write(
        app.join("product.json"),
        r#"{"sharedDataFolderName":".code-example-shared"}"#,
    )
    .unwrap();
    let sources = Sources::vscode(&fixture.0, Some(&fixture.0.join("Code.app")));
    assert_eq!(
        sources.databases[0],
        fixture
            .0
            .join(".code-example-shared/sharedStorage/state.vscdb")
    );
    assert_eq!(
        sources.databases[1],
        fixture
            .0
            .join("Library/Application Support/Code/User/globalStorage/state.vscdb")
    );
    fs::write(
        app.join("product.json"),
        r#"{"sharedDataFolderName":"../../somewhere"}"#,
    )
    .unwrap();
    assert_eq!(
        Sources::vscode(&fixture.0, Some(&fixture.0.join("Code.app"))).databases[0],
        fixture.0.join(".vscode-shared/sharedStorage/state.vscdb")
    );
}

#[test]
fn history_is_bounded_and_non_projects_are_excluded() {
    let fixture = Fixture::new();
    let sources = fixture.sources();
    let db = database(&sources.databases[0]);
    let entries: Vec<_> = (0..MAX_PROJECTS + 10)
        .map(|i| json!({"folderUri":format!("file:///example/project-{i}")}))
        .collect();
    save(&db, "recently.opened", json!(entries));
    let mut cache = Cache::default();
    cache.refresh(&sources, false);
    assert_eq!(cache.projects.len(), MAX_PROJECTS);
    assert_eq!(cache.projects[0].name, "project-0");
    assert_eq!(
        cache.projects.last().unwrap().name,
        format!("project-{}", MAX_PROJECTS - 1)
    );
}
