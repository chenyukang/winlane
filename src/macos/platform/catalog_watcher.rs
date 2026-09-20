use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SETTLE_TIME: Duration = Duration::from_millis(500);

pub struct CatalogWatcher {
    watcher: RecommendedWatcher,
    roots: Vec<PathBuf>,
    watched: Vec<(PathBuf, RecursiveMode)>,
    pending: Arc<Mutex<Option<Instant>>>,
}

impl CatalogWatcher {
    pub fn new(roots: Vec<PathBuf>, wake: impl Fn() + Send + 'static) -> notify::Result<Self> {
        let roots: Vec<_> = roots.into_iter().map(normalize_path).collect();
        let pending = Arc::new(Mutex::new(None));
        let changed = pending.clone();
        let observed = roots.clone();
        let watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let relevant = match event {
                Ok(event) => affects_catalog(&event, &observed),
                Err(error) => {
                    eprintln!("Application directory notification failed: {error}");
                    true
                }
            };
            if relevant {
                // Keep one timestamp instead of queueing every file copied by an installer.
                let first = changed.lock().unwrap().replace(Instant::now()).is_none();
                if first {
                    wake();
                }
            }
        })?;
        let mut monitor = Self {
            watcher,
            roots,
            watched: Vec::new(),
            pending,
        };
        monitor.update_watches();
        if monitor.watched.is_empty() {
            return Err(notify::Error::generic(
                "No application directory could be monitored",
            ));
        }
        Ok(monitor)
    }

    pub fn take_changed(&mut self) -> bool {
        let mut pending = self.pending.lock().unwrap();
        if pending.is_none_or(|at| at.elapsed() < SETTLE_TIME) {
            return false;
        }
        pending.take();
        drop(pending);
        self.update_watches();
        true
    }

    fn update_watches(&mut self) {
        let mut wanted = Vec::new();
        for root in &self.roots {
            // A missing ~/Applications needs its parent watched until it is created.
            let Some(path) = root.ancestors().find(|path| path.is_dir()) else {
                continue;
            };
            let mode = if path == root {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            let target = (path.to_path_buf(), mode);
            if !wanted.contains(&target) {
                wanted.push(target);
            }
        }
        if wanted == self.watched {
            return;
        }
        for (path, _) in self.watched.drain(..) {
            let _ = self.watcher.unwatch(&path);
        }
        for (path, mode) in wanted {
            match self.watcher.watch(&path, mode) {
                Ok(()) => self.watched.push((path, mode)),
                Err(error) => eprintln!(
                    "Could not monitor application directory {}: {error}",
                    path.display()
                ),
            }
        }
    }
}

fn normalize_path(path: PathBuf) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => normalize_path(parent.to_path_buf()).join(name),
        _ => path,
    }
}

fn affects_catalog(event: &Event, roots: &[PathBuf]) -> bool {
    event.need_rescan()
        || (!matches!(event.kind, EventKind::Access(_))
            && (event.paths.is_empty()
                || event.paths.iter().any(|path| {
                    roots
                        .iter()
                        .any(|root| path.starts_with(root) || root.starts_with(path))
                })))
}

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../tests/native/catalog_watcher.rs"]
pub(crate) mod tests;
