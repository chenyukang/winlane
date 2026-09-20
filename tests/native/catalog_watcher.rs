use super::*;
use notify::event::{AccessKind, Flag};
use std::sync::mpsc;

pub fn verify() {
    let root = PathBuf::from("/Applications");
    for path in [
        "/Applications/Editor.app",
        "/Applications/Tools/Editor.app/Contents/Info.plist",
        "/Applications/Tools",
    ] {
        assert!(affects_catalog(
            &Event::new(EventKind::Any).add_path(path.into()),
            std::slice::from_ref(&root)
        ));
    }
    assert!(!affects_catalog(
        &Event::new(EventKind::Any).add_path("/tmp/unrelated.txt".into()),
        std::slice::from_ref(&root)
    ));
    assert!(!affects_catalog(
        &Event::new(EventKind::Access(AccessKind::Read)).add_path(root.clone()),
        std::slice::from_ref(&root)
    ));
    assert!(affects_catalog(
        &Event::new(EventKind::Other).set_flag(Flag::Rescan),
        &[root]
    ));

    let directory = tempfile::tempdir().unwrap();
    let root = directory
        .path()
        .canonicalize()
        .unwrap()
        .join("Applications");
    let (tx, rx) = mpsc::channel();
    let mut watcher = CatalogWatcher::new(vec![root.clone()], move || {
        let _ = tx.send(());
    })
    .unwrap();
    std::fs::create_dir(&root).unwrap();
    wait_for_change(&mut watcher, &rx);
    assert!(
        watcher
            .watched
            .contains(&(root.clone(), RecursiveMode::Recursive))
    );

    let app = root.join("Tools/Example.app");
    std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    std::fs::write(app.join("Contents/Info.plist"), "fixture").unwrap();
    wait_for_change(&mut watcher, &rx);
    let renamed = app.with_file_name("Renamed.app");
    std::fs::rename(&app, &renamed).unwrap();
    wait_for_change(&mut watcher, &rx);
    std::fs::remove_dir_all(&renamed).unwrap();
    wait_for_change(&mut watcher, &rx);
    std::fs::remove_dir_all(&root).unwrap();
    wait_for_change(&mut watcher, &rx);
    assert!(watcher.watched.contains(&(
        root.parent().unwrap().to_path_buf(),
        RecursiveMode::NonRecursive
    )));
    std::fs::create_dir(&root).unwrap();
    wait_for_change(&mut watcher, &rx);
    assert!(watcher.watched.contains(&(root, RecursiveMode::Recursive)));
    drop(watcher);
    println!(
        "Application notifications: real FSEvents detect install, rename, removal, nested apps and a recreated Applications directory; only temporary files changed."
    );
}

fn wait_for_change(watcher: &mut CatalogWatcher, events: &mpsc::Receiver<()>) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !watcher.take_changed() {
        assert!(
            Instant::now() < deadline,
            "application directory notification timed out"
        );
        let _ = events.recv_timeout(Duration::from_millis(25));
    }
}
