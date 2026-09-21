use objc2::AnyThread;
use objc2_foundation::{NSDate, NSDateFormatter, NSLocale, NSString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::time::{Duration, SystemTime};

static LOGGER: OnceLock<Logger> = OnceLock::new();
const FILE_LIMIT: u64 = 2 * 1024 * 1024;
const MESSAGE_LIMIT: usize = 16 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Debug,
    Info,
    Warn,
}
struct Event {
    time: SystemTime,
    level: Level,
    component: String,
    event: String,
    data: String,
    dropped: u64,
}
enum Message {
    Event(Event),
    Flush(mpsc::Sender<()>),
}
struct Logger {
    sender: SyncSender<Message>,
    debug: AtomicBool,
    dropped: AtomicU64,
}

pub fn directory() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Logs/Winlane"))
}
pub fn init(debug: bool) {
    if let Some(directory) = directory() {
        LOGGER.get_or_init(|| Logger::start(directory, debug));
        record(Level::Info, "app", "start", || {
            format!("version={} debug={debug}", env!("CARGO_PKG_VERSION"))
        });
    }
}
pub fn set_debug(enabled: bool) {
    if let Some(logger) = LOGGER.get()
        && logger.debug.swap(enabled, Ordering::Relaxed) != enabled
    {
        record(Level::Info, "logging", "debug-mode", || {
            format!("enabled={enabled}")
        });
    }
}
pub fn record(level: Level, component: &str, event: &str, data: impl FnOnce() -> String) {
    if let Some(logger) = LOGGER.get() {
        logger.record(level, component, event, data);
    }
}
pub fn flush() {
    if let Some(logger) = LOGGER.get() {
        logger.flush();
    }
}
impl Logger {
    fn start(directory: PathBuf, debug: bool) -> Self {
        let (sender, receiver) = mpsc::sync_channel(256);
        std::thread::spawn(move || {
            let result = (|| -> io::Result<()> {
                fs::create_dir_all(&directory)?;
                let mut writer = LogWriter::new(directory, FILE_LIMIT)?;
                for message in receiver {
                    match message {
                        Message::Event(event) => {
                            objc2::rc::autoreleasepool(|_| writer.write_event(event))?
                        }
                        Message::Flush(reply) => {
                            writer.file.flush()?;
                            let _ = reply.send(());
                        }
                    }
                }
                Ok(())
            })();
            if let Err(error) = result {
                eprintln!("Winlane logging stopped: {error}");
            }
        });
        Self {
            sender,
            debug: AtomicBool::new(debug),
            dropped: AtomicU64::new(0),
        }
    }
    fn record(&self, level: Level, component: &str, event: &str, data: impl FnOnce() -> String) {
        if level == Level::Debug && !self.debug.load(Ordering::Relaxed) {
            return;
        }
        let dropped = self.dropped.swap(0, Ordering::Relaxed);
        let message = Message::Event(Event {
            time: SystemTime::now(),
            level,
            component: component.into(),
            event: event.into(),
            data: bounded(data()),
            dropped,
        });
        // Never wait for disk I/O in a shortcut, focus callback, or cleanup operation.
        if self.sender.try_send(message).is_err() {
            self.dropped.fetch_add(dropped + 1, Ordering::Relaxed);
        }
    }
    fn flush(&self) {
        let (tx, rx) = mpsc::channel();
        if self.sender.try_send(Message::Flush(tx)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_millis(250));
        }
    }
}
fn bounded(mut text: String) -> String {
    if text.len() > MESSAGE_LIMIT {
        let mut end = MESSAGE_LIMIT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str(" [truncated]");
    }
    text
}
struct LogWriter {
    file: File,
    path: PathBuf,
    backup: PathBuf,
    bytes: u64,
    limit: u64,
}
impl LogWriter {
    fn new(directory: PathBuf, limit: u64) -> io::Result<Self> {
        let path = directory.join("winlane.log");
        let file = Self::open(&path)?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            file,
            path,
            backup: directory.join("winlane.previous.log"),
            bytes,
            limit,
        })
    }
    fn open(path: &PathBuf) -> io::Result<File> {
        use std::os::unix::fs::PermissionsExt;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(path)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        Ok(file)
    }
    fn write_event(&mut self, event: Event) -> io::Result<()> {
        let formatter = NSDateFormatter::new();
        formatter.setLocale(Some(&NSLocale::initWithLocaleIdentifier(
            NSLocale::alloc(),
            &NSString::from_str("en_US_POSIX"),
        )));
        formatter.setDateFormat(Some(&NSString::from_str("yyyy-MM-dd'T'HH:mm:ss.SSSZZZZZ")));
        let date = NSDate::dateWithTimeIntervalSince1970(
            event
                .time
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64(),
        );
        let time = formatter.stringFromDate(&date);
        self.write(&format!(
            "{} {:?} pid={} component={:?} event={:?} dropped={} data={:?}\n",
            time,
            event.level,
            std::process::id(),
            event.component,
            event.event,
            event.dropped,
            event.data
        ))
    }
    fn write(&mut self, line: &str) -> io::Result<()> {
        if self.bytes > 0 && self.bytes.saturating_add(line.len() as u64) > self.limit {
            fs::rename(&self.path, &self.backup)?;
            self.file = Self::open(&self.path)?;
            self.bytes = 0;
        }
        self.file.write_all(line.as_bytes())?;
        self.bytes += line.len() as u64;
        Ok(())
    }
}
