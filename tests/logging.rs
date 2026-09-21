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
    let mut writer = LogWriter::new(directory.0.join("winlane.log"), 100).unwrap();
    writer.write("old process\n").unwrap();
    drop(writer);
    let mut writer = LogWriter::new(directory.0.join("winlane.log"), 100).unwrap();
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
    let mut writer = LogWriter::new(directory.0.join("winlane.log"), 20).unwrap();
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
fn live_level_switch_and_flush_preserve_info_and_escape_newlines() {
    let directory = tempfile::tempdir().unwrap();
    let logger = Logger::start(directory.path().join("winlane.log"), Level::Info);
    logger.record(Level::Debug, "test", "off", || panic!("must be lazy"));
    logger.record(Level::Info, "auto-appclose", "close-request", || {
        "app=example\nforged entry".into()
    });
    logger
        .configure(directory.path().join("winlane.log"), Level::Debug)
        .unwrap();
    logger.record(Level::Debug, "test", "on", || "details".into());
    logger.flush();
    let log = fs::read_to_string(directory.path().join("winlane.log")).unwrap();
    assert_eq!(log.lines().count(), 2);
    assert!(log.contains("close-request"));
    assert!(log.contains("Debug"));
    assert!(!log.contains("event=\"off\""));
    logger
        .configure(directory.path().join("winlane.log"), Level::Info)
        .unwrap();
    logger.record(Level::Debug, "test", "off-again", || panic!("must be lazy"));
}

#[test]
fn oversized_unicode_events_are_bounded_without_breaking_utf8() {
    let text = bounded("日".repeat(MESSAGE_LIMIT));
    assert!(text.len() < MESSAGE_LIMIT + 20);
    assert!(text.ends_with(" [truncated]"));
}

#[test]
fn each_threshold_filters_before_formatting() {
    for threshold in [
        Level::Off,
        Level::Error,
        Level::Warn,
        Level::Info,
        Level::Debug,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("level.log");
        let logger = Logger::start(path.clone(), threshold);
        for event in [Level::Error, Level::Warn, Level::Info, Level::Debug] {
            logger.record(event, "test", "level", || {
                assert!(threshold.allows(event), "filtered payload was formatted");
                "details".into()
            });
        }
        logger.flush();
        let log = fs::read_to_string(path).unwrap_or_default();
        let expected = match threshold {
            Level::Off => 0,
            Level::Error => 1,
            Level::Warn => 2,
            Level::Info => 3,
            Level::Debug => 4,
        };
        assert_eq!(log.lines().count(), expected);
    }
}

#[test]
fn path_switch_routes_queued_events_and_unwritable_path_keeps_old_settings() {
    let directory = tempfile::tempdir().unwrap();
    let old = directory.path().join("original.log");
    let new = directory.path().join("nested/custom.log");
    let logger = Logger::start(old.clone(), Level::Info);
    logger.record(Level::Info, "test", "before", String::new);
    logger.configure(new.clone(), Level::Debug).unwrap();
    logger.record(Level::Debug, "test", "after", String::new);
    logger.flush();
    assert!(fs::read_to_string(old).unwrap().contains("before"));
    let log = fs::read_to_string(&new).unwrap();
    assert!(!log.contains("before"));
    assert!(log.contains("after"));

    let blocked = directory.path().join("not-a-directory");
    fs::write(&blocked, "keep this file").unwrap();
    assert!(
        logger
            .configure(blocked.join("bad.log"), Level::Off)
            .is_err()
    );
    assert!(
        logger
            .configure(directory.path().into(), Level::Off)
            .is_err()
    );
    assert!(logger.configure("/dev/null".into(), Level::Off).is_err());
    logger.record(Level::Debug, "test", "still-active", String::new);
    logger.flush();
    assert!(fs::read_to_string(new).unwrap().contains("still-active"));
    assert_eq!(fs::read_to_string(blocked).unwrap(), "keep this file");
}

#[test]
fn custom_filename_rotation_stays_in_the_configured_directory() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("nested/custom.log");
    let mut writer = LogWriter::new(path.clone(), 10).unwrap();
    writer.write("previous\n").unwrap();
    writer.write("current\n").unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), "current\n");
    assert_eq!(
        fs::read_to_string(directory.path().join("nested/custom.previous.log")).unwrap(),
        "previous\n"
    );
}
