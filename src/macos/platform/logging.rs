use objc2::AnyThread;
use objc2_foundation::{NSDate, NSDateFormatter, NSLocale, NSString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};
pub use winlane::core::logging::Level;
use winlane::core::logging::Settings;

static LOGGER: OnceLock<Logger> = OnceLock::new();
const FILE_LIMIT: u64 = 2 * 1024 * 1024;
const MESSAGE_LIMIT: usize = 16 * 1024;

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
    Configure(LogWriter),
}
struct Logger {
    sender: SyncSender<Message>,
    destination: Mutex<Destination>,
    dropped: AtomicU64,
}

struct Destination {
    path: PathBuf,
    level: Level,
}

fn file_path(settings: &Settings) -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    settings.resolve_path(home.as_deref())
}
pub fn directory(settings: &Settings) -> Result<PathBuf, String> {
    Ok(file_path(settings)?.parent().unwrap().to_path_buf())
}
pub fn init(settings: &Settings) {
    match file_path(settings) {
        Ok(path) => {
            LOGGER.get_or_init(|| Logger::start(path, settings.level));
            record(Level::Info, "app", "start", || {
                format!(
                    "version={} log_level={:?}",
                    env!("CARGO_PKG_VERSION"),
                    settings.level
                )
            });
        }
        Err(error) => eprintln!("Winlane logging could not start: {error}"),
    }
}
pub fn configure(settings: &Settings) -> Result<(), String> {
    if let Some(logger) = LOGGER.get() {
        logger
            .configure(file_path(settings)?, settings.level)
            .map_err(|error| {
                winlane::trf!(
                    "无法应用日志设置：{}",
                    "Could not apply logging settings: {}",
                    error
                )
            })?;
    }
    Ok(())
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
    fn start(path: PathBuf, level: Level) -> Self {
        let (sender, receiver) = mpsc::sync_channel(256);
        let mut worker_path = path.clone();
        std::thread::spawn(move || {
            let mut writer: Option<LogWriter> = None;
            let mut error_reported = false;
            for message in receiver {
                let result = match message {
                    Message::Event(event) => (|| -> io::Result<()> {
                        if writer.is_none() {
                            writer = Some(LogWriter::new(worker_path.clone(), FILE_LIMIT)?);
                        }
                        objc2::rc::autoreleasepool(|_| writer.as_mut().unwrap().write_event(event))
                    })(),
                    Message::Flush(reply) => {
                        let result = writer.as_mut().map_or(Ok(()), |writer| writer.file.flush());
                        let _ = reply.send(());
                        result
                    }
                    Message::Configure(new_writer) => {
                        worker_path = new_writer.path.clone();
                        writer = Some(new_writer);
                        error_reported = false;
                        Ok(())
                    }
                };
                if let Err(error) = result {
                    if !error_reported {
                        eprintln!("Winlane logging failed: {error}");
                        error_reported = true;
                    }
                    writer = None;
                }
            }
        });
        Self {
            sender,
            destination: Mutex::new(Destination { path, level }),
            dropped: AtomicU64::new(0),
        }
    }
    fn configure(&self, path: PathBuf, level: Level) -> io::Result<()> {
        let changed_path = self.destination.lock().unwrap().path != path;
        // Open the new destination before switching or saving settings. A failed path
        // must leave the old log and severity threshold intact.
        let writer = if changed_path {
            Some(LogWriter::new(path.clone(), FILE_LIMIT)?)
        } else {
            None
        };
        let mut destination = self.destination.lock().unwrap();
        if let Some(writer) = writer {
            self.sender
                .try_send(Message::Configure(writer))
                .map_err(|_| {
                    io::Error::other(winlane::tr!(
                        "日志队列正忙，请稍后重试。",
                        "The logging queue is busy. Try again shortly."
                    ))
                })?;
        }
        *destination = Destination { path, level };
        Ok(())
    }
    fn record(&self, level: Level, component: &str, event: &str, data: impl FnOnce() -> String) {
        if !self.destination.lock().unwrap().level.allows(level) {
            return;
        }
        let data = bounded(data());
        let destination = self.destination.lock().unwrap();
        if !destination.level.allows(level) {
            return;
        }
        let dropped = self.dropped.swap(0, Ordering::Relaxed);
        let message = Message::Event(Event {
            time: SystemTime::now(),
            level,
            component: component.into(),
            event: event.into(),
            data,
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
    fn new(path: PathBuf, limit: u64) -> io::Result<Self> {
        fs::create_dir_all(
            path.parent()
                .ok_or_else(|| io::Error::other("Missing log directory"))?,
        )?;
        let mut backup_name = path
            .file_stem()
            .ok_or_else(|| io::Error::other("Missing log filename"))?
            .to_os_string();
        backup_name.push(".previous");
        if let Some(extension) = path.extension() {
            backup_name.push(".");
            backup_name.push(extension);
        }
        let backup = path.with_file_name(backup_name);
        let file = Self::open(&path)?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            file,
            path,
            backup,
            bytes,
            limit,
        })
    }
    fn open(path: &PathBuf) -> io::Result<File> {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(path)
            && !metadata.is_file()
        {
            return Err(io::Error::other(winlane::tr!(
                "日志路径必须指向普通文件。",
                "The log path must point to a regular file."
            )));
        }
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
