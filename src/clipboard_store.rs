use crate::clipboard::{ClipboardSettings, History};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::JoinHandle;

#[derive(Default)]
struct Pending {
    snapshot: Option<Option<History>>,
    reset: bool,
    stop: bool,
}
pub struct Store {
    pending: Arc<(Mutex<Pending>, Condvar)>,
    worker: Option<JoinHandle<()>>,
    pub loaded: mpsc::Receiver<Result<History, String>>,
    pub errors: mpsc::Receiver<String>,
}
impl Store {
    pub fn new(path: PathBuf, now: u64, settings: ClipboardSettings) -> Self {
        let pending = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let work = pending.clone();
        let (loaded_tx, loaded) = mpsc::channel();
        let (error_tx, errors) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let initial = if settings.persistent {
                read(&path, now, &settings)
            } else {
                write(&path, None).map(|_| History::default())
            };
            let mut writable = initial.is_ok();
            let _ = loaded_tx.send(initial.map_err(|error| error.to_string()));
            loop {
                let (mutex, wake) = &*work;
                let mut pending = mutex.lock().unwrap();
                while pending.snapshot.is_none() && !pending.stop {
                    pending = wake.wait(pending).unwrap();
                }
                let snapshot = pending.snapshot.take();
                let reset = std::mem::take(&mut pending.reset);
                let stop = pending.stop;
                drop(pending);
                if reset {
                    match write(&path, None) {
                        Ok(()) => writable = true,
                        Err(error) => {
                            let _ = error_tx.send(error.to_string());
                        }
                    }
                }
                if let Some(snapshot) = snapshot {
                    if writable || snapshot.is_none() {
                        match write(&path, snapshot.as_ref()) {
                            Ok(()) => writable = true,
                            Err(error) => {
                                let _ = error_tx.send(error.to_string());
                            }
                        }
                    }
                } else if stop {
                    break;
                }
            }
        });
        Self {
            pending,
            worker: Some(worker),
            loaded,
            errors,
        }
    }
    pub fn save(&self, history: Option<History>) {
        let (mutex, wake) = &*self.pending;
        let mut pending = mutex.lock().unwrap();
        pending.reset |= history.is_none();
        pending.snapshot = Some(history);
        drop(pending);
        wake.notify_one();
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let (mutex, wake) = &*self.pending;
        mutex.lock().unwrap().stop = true;
        wake.notify_one();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn read(path: &Path, now: u64, settings: &ClipboardSettings) -> Result<History, std::io::Error> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(History::default()),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(32 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(std::io::Error::other(
            "Clipboard history file exceeds its size limit",
        ));
    }
    History::from_json(&bytes, now, settings).map_err(std::io::Error::other)
}
fn write(path: &Path, history: Option<&History>) -> Result<(), std::io::Error> {
    let Some(history) = history else {
        return match fs::remove_file(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            result => result,
        };
    };
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("Missing history directory"))?;
    fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp = path.with_extension(format!("{}.{stamp}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temp)?;
        serde_json::to_writer(&mut file, history)?;
        file.flush()?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
