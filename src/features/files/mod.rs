//! File search policy, ranking and a small local recently-opened list.
use crate::{tr, trf};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

pub mod history;
pub const MAX_RESULTS: usize = 50;
pub const MAX_CANDIDATES: usize = 5000;
pub const MAX_RECENT: usize = 25;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub roots: Vec<String>,
    pub excluded: Vec<String>,
    pub hide_generated: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            roots: vec!["~".into()],
            excluded: vec!["~/Library".into()],
            hide_generated: true,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.roots.is_empty() || self.roots.len() > 16 || self.excluded.len() > 32 {
            return Err(tr!(
                "请选择 1–16 个搜索目录，最多排除 32 个目录。",
                "Choose 1–16 search folders and at most 32 excluded folders."
            )
            .into());
        }
        for path in self.roots.iter().chain(&self.excluded) {
            if path.len() > 4096 || expand(path, Path::new("/home")).is_none() {
                return Err(tr!(
                    "目录必须是绝对路径或以 ~/ 开头。",
                    "Folders must be absolute paths or start with ~/."
                )
                .into());
            }
        }
        Ok(())
    }
    pub fn roots(&self, home: &Path) -> Vec<PathBuf> {
        self.roots
            .iter()
            .filter_map(|path| expand(path, home))
            .collect()
    }
    pub fn allows(&self, path: &Path, home: &Path) -> bool {
        self.roots(home).iter().any(|root| path.starts_with(root))
            && !self
                .excluded
                .iter()
                .filter_map(|p| expand(p, home))
                .any(|p| path.starts_with(p))
            && !hidden_or_generated(path, self.hide_generated)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub directory: bool,
    pub modified: u64,
}
impl Entry {
    pub fn read(path: PathBuf) -> std::io::Result<Self> {
        let meta = std::fs::metadata(&path)?;
        if !meta.is_file() && !meta.is_dir() {
            return Err(std::io::Error::other("Not a regular file or folder"));
        }
        Ok(Self {
            name: path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned(),
            path,
            directory: meta.is_dir(),
            modified: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs()),
        })
    }
    pub fn parent_label(&self, home: &Path) -> String {
        let parent = self.path.parent().unwrap_or(&self.path);
        if let Ok(relative) = parent.strip_prefix(home) {
            if relative.as_os_str().is_empty() {
                "~".into()
            } else {
                format!("~/{}", relative.display())
            }
        } else {
            parent.to_string_lossy().into_owned()
        }
    }
    pub fn completion(&self, home: &Path) -> String {
        let mut path = match self.path.strip_prefix(home) {
            Ok(relative) if relative.as_os_str().is_empty() => "~".into(),
            Ok(relative) => format!("~/{}", relative.display()),
            Err(_) => self.path.to_string_lossy().into_owned(),
        };
        if self.directory && !path.ends_with('/') {
            path.push('/');
        }
        path
    }
    pub fn symbol(&self) -> &'static str {
        if self.directory {
            return "folder";
        }
        match self
            .path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "png" | "jpg" | "jpeg" | "gif" | "heic" | "webp" | "svg" => "photo",
            "pdf" => "doc.richtext",
            "mp3" | "m4a" | "wav" | "flac" => "music.note",
            "mp4" | "mov" | "mkv" => "film",
            "zip" | "gz" | "tar" | "7z" => "archivebox",
            _ => "doc.text",
        }
    }
}

pub fn expand(value: &str, home: &Path) -> Option<PathBuf> {
    let value = value.trim();
    let path = if value == "~" {
        home.to_path_buf()
    } else if let Some(tail) = value.strip_prefix("~/") {
        home.join(tail)
    } else if value.starts_with('/') {
        PathBuf::from(value)
    } else {
        return None;
    };
    let mut normalized = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    Some(normalized)
}

/// An explicit trailing slash browses a folder; otherwise search its leaf name.
pub fn path_query(query: &str, home: &Path) -> Option<(PathBuf, String)> {
    let path = expand(query, home)?;
    if query.trim().ends_with('/') || query.trim() == "~" {
        return Some((path, String::new()));
    }
    Some((
        path.parent()?.to_path_buf(),
        path.file_name()?.to_string_lossy().into_owned(),
    ))
}

/// Remove the final typed path component, keeping home and filesystem roots intact.
pub fn parent_query(query: &str) -> Option<String> {
    let query = query.trim();
    if query != "~" && !query.starts_with("~/") && !query.starts_with('/') {
        return None;
    }
    let path = query.trim_end_matches('/');
    if path == "~" {
        return Some("~/".into());
    }
    let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
    Some(format!("{}/", parent.trim_end_matches('/')))
}

pub fn hidden_or_generated(path: &Path, generated: bool) -> bool {
    path.components().any(|c| {
        let name = c.as_os_str().to_string_lossy();
        name.starts_with('.')
            || name.ends_with(".app")
            || (generated
                && matches!(
                    name.as_ref(),
                    "node_modules" | "target" | "__pycache__" | "DerivedData" | "build" | "dist"
                ))
    })
}

fn subsequence(needle: &str, haystack: &str) -> bool {
    let mut chars = haystack.chars();
    needle.chars().all(|c| chars.by_ref().any(|h| c == h))
}

pub fn score(entry: &Entry, query: &str) -> Option<u8> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(0);
    }
    let name = entry.name.to_lowercase();
    let stem = Path::new(&name)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    if name == query || stem == query {
        return Some(0);
    }
    if name.starts_with(&query) {
        return Some(1);
    }
    if name.contains(&query) {
        return Some(2);
    }
    let parent = entry
        .path
        .parent()
        .unwrap_or(Path::new(""))
        .to_string_lossy()
        .to_lowercase();
    let mut rank = 2;
    for word in query.split_whitespace() {
        if name.contains(word) {
            continue;
        }
        if parent.contains(word) {
            rank = rank.max(3);
        } else if word.chars().count() >= if entry.directory { 2 } else { 3 }
            && subsequence(word, &name)
        {
            rank = 4;
        } else {
            return None;
        }
    }
    Some(rank)
}

pub fn matching(entries: &[Entry], query: &str, recent: &[Entry]) -> Vec<Entry> {
    let mut seen = HashSet::new();
    let mut ranked: Vec<_> = entries
        .iter()
        .filter(|e| seen.insert(&e.path))
        .filter_map(|e| {
            score(e, query).map(|rank| {
                (
                    rank,
                    recent
                        .iter()
                        .position(|r| r.path == e.path)
                        .unwrap_or(usize::MAX),
                    e,
                )
            })
        })
        .collect();
    ranked.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.cmp(&b.1))
            .then(b.2.modified.cmp(&a.2.modified))
            .then(a.2.name.cmp(&b.2.name))
            .then(a.2.path.cmp(&b.2.path))
    });
    ranked
        .into_iter()
        .take(MAX_RESULTS)
        .map(|(_, _, e)| e.clone())
        .collect()
}

pub fn remember(recent: &mut Vec<Entry>, entry: Entry) {
    recent.retain(|e| e.path != entry.path);
    recent.insert(0, entry);
    recent.truncate(MAX_RECENT);
}

pub fn browse(
    query: &str,
    home: &Path,
    cancelled: impl Fn() -> bool,
) -> Result<Vec<Entry>, String> {
    let (directory, leaf) =
        path_query(query, home).ok_or_else(|| tr!("路径无效。", "Invalid path.").to_owned())?;
    let reader = std::fs::read_dir(&directory).map_err(|e| {
        trf!(
            "无法读取目录 {}：{}",
            "Cannot read folder {}: {}",
            directory.display(),
            e
        )
    })?;
    let mut entries = Vec::new();
    for item in reader {
        if cancelled() {
            break;
        }
        let Ok(item) = item else {
            continue;
        };
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') && !leaf.starts_with('.') {
            continue;
        }
        if let Ok(entry) = Entry::read(item.path())
            && score(&entry, &leaf).is_some()
        {
            entries.push(entry);
        }
        if entries.len() >= MAX_CANDIDATES {
            break;
        }
    }
    Ok(entries)
}
