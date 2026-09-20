use super::{Entry, MAX_RECENT};
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
struct History {
    version: u8,
    entries: Vec<Entry>,
}
pub fn path(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Winlane/Files/recent.json")
}
pub fn load(path: &Path) -> io::Result<Vec<Entry>> {
    let mut data = Vec::new();
    std::fs::File::open(path)?
        .take(256 * 1024 + 1)
        .read_to_end(&mut data)?;
    if data.len() > 256 * 1024 {
        return Err(io::Error::other("File history exceeds size limit"));
    }
    let history: History = serde_json::from_slice(&data)?;
    if history.version != 1 {
        return Err(io::Error::other("Unknown file history version"));
    }
    let mut seen = std::collections::HashSet::new();
    Ok(history
        .entries
        .into_iter()
        .filter(|e| e.path.is_absolute() && seen.insert(e.path.clone()))
        .filter_map(|e| Entry::read(e.path).ok())
        .take(MAX_RECENT)
        .collect())
}
pub fn save(path: &Path, entries: &[Entry]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing history directory"))?;
    std::fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
    }
    let history = History {
        version: 1,
        entries: entries.iter().take(MAX_RECENT).cloned().collect(),
    };
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut temporary, &history)?;
    temporary.flush()?;
    temporary.persist(path)?;
    Ok(())
}
