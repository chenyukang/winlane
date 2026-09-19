use super::{MAX_RESULTS, Project};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const MAX_SNAPSHOT_BYTES: u64 = 256 * 1024;

#[derive(Serialize, Deserialize)]
struct Snapshot {
    version: u8,
    projects: Vec<Project>,
}

pub fn path(home: &Path) -> PathBuf {
    home.join("Library/Caches/app.windowlane.desktop/projects.json")
}

pub fn load(path: &Path) -> io::Result<Vec<Project>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_SNAPSHOT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(io::Error::other("Project snapshot exceeds size limit"));
    }
    let snapshot: Snapshot = serde_json::from_slice(&bytes)?;
    if snapshot.version != 1 {
        return Err(io::Error::other("Unsupported project snapshot version"));
    }
    Ok(recent(&snapshot.projects))
}

pub fn save(path: &Path, projects: &[Project]) -> io::Result<()> {
    let snapshot = Snapshot {
        version: 1,
        projects: recent(projects),
    };
    let bytes = serde_json::to_vec(&snapshot)?;
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(io::Error::other("Project snapshot exceeds size limit"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing cache directory"))?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&bytes)?;
    file.flush()?;
    file.persist(path)?;
    Ok(())
}

fn recent(projects: &[Project]) -> Vec<Project> {
    let mut seen = HashSet::new();
    projects
        .iter()
        .filter(|project| {
            project.path.is_absolute()
                && !project.name.trim().is_empty()
                && seen.insert(&project.path)
        })
        .take(MAX_RESULTS)
        .cloned()
        .collect()
}
