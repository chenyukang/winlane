use super::{Entry, MAX_RESULTS, hidden_or_generated, query::Matcher};
use crate::{tr, trf};
use std::collections::VecDeque;
use std::path::Path;
use std::time::{Duration, Instant};

pub struct Listing {
    pub entries: Vec<Entry>,
    pub limited: bool,
}

/// Walk explicit paths on the search worker, without depending on Spotlight indexing.
pub fn search(
    matcher: &Matcher,
    recent: &[Entry],
    hide_generated: bool,
    cancelled: impl Fn() -> bool,
    mut progress: impl FnMut(&[Entry]),
) -> Result<Listing, String> {
    let root = matcher
        .directory
        .as_ref()
        .ok_or_else(|| tr!("路径无效。", "Invalid path.").to_owned())?;
    let recursive = matcher.recursive();
    let started = Instant::now();
    let mut last_update = started;
    let mut pending = VecDeque::from([(root.clone(), 0)]);
    let mut entries = Vec::new();
    let mut visited = 0;
    let mut limited = false;
    'folders: while let Some((directory, depth)) = pending.pop_front() {
        if cancelled() {
            break;
        }
        if visited >= 100_000 || started.elapsed() >= Duration::from_secs(3) {
            limited = true;
            break;
        }
        let reader = match std::fs::read_dir(&directory) {
            Ok(reader) => reader,
            Err(error) if directory == *root => {
                return Err(trf!(
                    "无法读取目录 {}：{}",
                    "Cannot read folder {}: {}",
                    directory.display(),
                    error
                ));
            }
            Err(_) => {
                limited = true;
                continue;
            }
        };
        for item in reader {
            if cancelled() {
                break 'folders;
            }
            if visited >= 100_000 || started.elapsed() >= Duration::from_secs(3) {
                limited = true;
                break 'folders;
            }
            visited += 1;
            let Ok(item) = item else {
                limited = true;
                continue;
            };
            let name = item.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') && (!matcher.term.starts_with('.') || recursive) {
                continue;
            }
            let Ok(kind) = item.file_type() else {
                limited = true;
                continue;
            };
            // Only enqueue actual directories, never symlinks that could escape the root or loop.
            if recursive && kind.is_dir() && !hidden_or_generated(Path::new(&name), hide_generated)
            {
                if depth < 128 {
                    pending.push_back((item.path(), depth + 1));
                } else {
                    limited = true;
                }
            }
            if let Ok(entry) = Entry::read(item.path())
                && matcher.score(&entry).is_some()
            {
                entries.push(entry);
                if entries.len() >= MAX_RESULTS * 2 {
                    entries = matcher.matching(&entries, recent);
                }
            }
            if last_update.elapsed() >= Duration::from_millis(100) {
                entries = matcher.matching(&entries, recent);
                progress(&entries);
                last_update = Instant::now();
            }
        }
    }
    Ok(Listing {
        entries: matcher.matching(&entries, recent),
        limited,
    })
}
