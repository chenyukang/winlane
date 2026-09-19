use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::time::SystemTime;

static SENDER: OnceLock<SyncSender<String>> = OnceLock::new();
static DROPPED: AtomicU64 = AtomicU64::new(0);
const FILE_LIMIT: u64 = 2 * 1024 * 1024;

pub fn init(directory: PathBuf) {
    let (sender, receiver) = mpsc::sync_channel::<String>(256);
    if SENDER.set(sender).is_err() {
        return;
    }
    std::thread::spawn(move || {
        let result = (|| -> io::Result<()> {
            fs::create_dir_all(&directory)?;
            let mut writer = TraceWriter::new(directory, FILE_LIMIT)?;
            for line in receiver {
                writer.write(&line)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("Window-order diagnostics stopped: {error}");
        }
    });
    record("start", || format!("version={}", env!("CARGO_PKG_VERSION")));
}

pub fn record(event: &str, data: impl FnOnce() -> String) {
    let Some(sender) = SENDER.get() else { return };
    let time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let dropped = DROPPED.swap(0, Ordering::Relaxed);
    let line = format!(
        "time={}.{:03} pid={} dropped={dropped} {event} {}\n",
        time.as_secs(),
        time.subsec_millis(),
        std::process::id(),
        data(),
    );
    // Diagnostic I/O must never hold up a shortcut or a focus callback.
    if sender.try_send(line).is_err() {
        DROPPED.fetch_add(dropped + 1, Ordering::Relaxed);
    }
}

struct TraceWriter {
    file: File,
    path: PathBuf,
    backup: PathBuf,
    bytes: u64,
    limit: u64,
}

impl TraceWriter {
    fn new(directory: PathBuf, limit: u64) -> io::Result<Self> {
        let path = directory.join("recency.log");
        let file = Self::open(&path)?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            file,
            path,
            backup: directory.join("recency.previous.log"),
            bytes,
            limit,
        })
    }

    fn open(path: &PathBuf) -> io::Result<File> {
        OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(path)
    }

    fn write(&mut self, line: &str) -> io::Result<()> {
        if self.bytes > 0 && self.bytes + line.len() as u64 > self.limit {
            fs::rename(&self.path, &self.backup)?;
            self.file = Self::open(&self.path)?;
            self.bytes = 0;
        }
        self.file.write_all(line.as_bytes())?;
        self.bytes += line.len() as u64;
        Ok(())
    }
}
