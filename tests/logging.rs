#![cfg(target_os = "macos")]
#![allow(dead_code)]

include!("../src/macos/platform/logging.rs");

struct Directory(PathBuf);

impl Directory {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("winlane-trace-test-{}-{name}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn disabled_diagnostics_do_not_format_events() {
    record(Level::Debug, "test", "disabled", || {
        panic!("disabled logging must not evaluate the payload")
    });
}

#[test]
fn history_appends_across_restarts_and_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let directory = Directory::new("restart");
    let mut writer = LogWriter::new(directory.0.clone(), 100).unwrap();
    writer.write("old process\n").unwrap();
    drop(writer);
    let mut writer = LogWriter::new(directory.0.clone(), 100).unwrap();
    writer.write("new process\n").unwrap();
    let path = directory.0.join("winlane.log");
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "old process\nnew process\n"
    );
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn rotation_keeps_recent_events_in_two_bounded_files() {
    let directory = Directory::new("rotation");
    let mut writer = LogWriter::new(directory.0.clone(), 20).unwrap();
    for i in 0..20 {
        writer.write(&format!("event {i:02}\n")).unwrap();
    }
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
    let previous = directory.0.join("winlane.previous.log");
    let current = directory.0.join("winlane.log");
    assert_eq!(
        fs::read_to_string(&previous).unwrap(),
        "event 16\nevent 17\n"
    );
    assert_eq!(
        fs::read_to_string(&current).unwrap(),
        "event 18\nevent 19\n"
    );
    assert!(fs::metadata(previous).unwrap().len() <= 20);
    assert!(fs::metadata(current).unwrap().len() <= 20);
}

#[test]
fn live_debug_switch_and_flush_preserve_info_and_escape_newlines() {
    let directory = tempfile::tempdir().unwrap();
    let logger = Logger::start(directory.path().into(), false);
    logger.record(Level::Debug, "test", "off", || panic!("must be lazy"));
    logger.record(Level::Info, "cleanup", "close-request", || {
        "app=example\nforged entry".into()
    });
    logger.debug.store(true, Ordering::Relaxed);
    logger.record(Level::Debug, "test", "on", || "details".into());
    logger.flush();
    let log = fs::read_to_string(directory.path().join("winlane.log")).unwrap();
    assert_eq!(log.lines().count(), 2);
    assert!(log.contains("close-request"));
    assert!(log.contains("Debug"));
    assert!(!log.contains("event=\"off\""));
    logger.debug.store(false, Ordering::Relaxed);
    logger.record(Level::Debug, "test", "off-again", || panic!("must be lazy"));
}

#[test]
fn oversized_unicode_events_are_bounded_without_breaking_utf8() {
    let text = bounded("日".repeat(MESSAGE_LIMIT));
    assert!(text.len() < MESSAGE_LIMIT + 20);
    assert!(text.ends_with(" [truncated]"));
}
