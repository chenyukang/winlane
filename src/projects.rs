use crate::{tr, trf};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_PROJECTS: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Folder,
    Workspace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub path: PathBuf,
    pub name: String,
    pub kind: Kind,
}

impl Project {
    pub fn validate_path(&self) -> Result<(), String> {
        let metadata = fs::metadata(&self.path).map_err(|_| {
            tr!("项目路径不可用，可能已移动、删除或未挂载。", "Project unavailable. It may have moved, been deleted, or be on an unmounted volume.").to_owned()
        })?;
        if match self.kind {
            Kind::Folder => metadata.is_dir(),
            Kind::Workspace => metadata.is_file(),
        } {
            Ok(())
        } else {
            Err(tr!(
                "项目路径的类型已改变。",
                "The project path has changed type."
            )
            .into())
        }
    }
}

#[derive(Clone, Debug)]
pub struct Sources {
    pub databases: Vec<PathBuf>,
    pub storage: PathBuf,
}

impl Sources {
    pub fn vscode(home: &Path, application: Option<&Path>) -> Self {
        let global = home.join("Library/Application Support/Code/User/globalStorage");
        let shared = application
            .and_then(|app| read_json(&app.join("Contents/Resources/app/product.json")).ok())
            .and_then(|value| {
                value
                    .get("sharedDataFolderName")?
                    .as_str()
                    .map(str::to_owned)
            })
            .filter(|name| {
                !name.is_empty()
                    && !matches!(name.as_str(), "." | "..")
                    && !name.contains(['/', '\\'])
            })
            .unwrap_or_else(|| ".vscode-shared".into());
        Self {
            databases: vec![
                home.join(shared).join("sharedStorage/state.vscdb"),
                global.join("state.vscdb"),
            ],
            storage: global.join("storage.json"),
        }
    }

    fn fingerprint(&self) -> Vec<Stamp> {
        self.databases
            .iter()
            .flat_map(|path| {
                let mut wal = path.as_os_str().to_os_string();
                wal.push("-wal");
                [path.clone(), PathBuf::from(wal)]
            })
            .chain(std::iter::once(self.storage.clone()))
            .map(|path| {
                let metadata = fs::metadata(&path).ok();
                Stamp {
                    path,
                    modified: metadata.as_ref().and_then(|m| m.modified().ok()),
                    length: metadata.map(|m| m.len()),
                }
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    path: PathBuf,
    modified: Option<SystemTime>,
    length: Option<u64>,
}

#[derive(Clone, Default)]
pub struct Cache {
    pub projects: Vec<Project>,
    pub error: Option<String>,
    fingerprint: Option<Vec<Stamp>>,
}

impl Cache {
    /// Called only on entry or explicit refresh, on a worker thread.
    pub fn refresh(&mut self, sources: &Sources, force: bool) -> bool {
        let before = sources.fingerprint();
        if !force && self.error.is_none() && self.fingerprint.as_ref() == Some(&before) {
            return false;
        }
        let (projects, error) = load(sources);
        if !projects.is_empty() || error.is_none() {
            self.projects = projects;
        }
        self.error = error;
        // A concurrent VS Code write must cause another read next time.
        self.fingerprint = (before == sources.fingerprint()).then_some(before);
        true
    }
}

pub fn matching(projects: &[Project], query: &str) -> Vec<Project> {
    let terms: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
    projects
        .iter()
        .filter(|project| {
            let text = format!("{} {}", project.name, project.path.display()).to_lowercase();
            terms.iter().all(|term| text.contains(term))
        })
        .cloned()
        .collect()
}

fn load(sources: &Sources) -> (Vec<Project>, Option<String>) {
    let mut projects = Vec::new();
    let mut seen = HashSet::new();
    let mut errors = Vec::new();
    for database in &sources.databases {
        match read_database(database) {
            Ok(Some(value)) => {
                if let Some(entries) = value.get("entries").and_then(Value::as_array) {
                    for entry in entries {
                        add_entry(entry, &mut projects, &mut seen);
                    }
                    break;
                }
                errors
                    .push(tr!("最近项目数据格式无效。", "Invalid recent-project data.").to_owned());
            }
            Ok(None) => {}
            Err(error) => errors.push(error),
        }
    }
    match read_json(&sources.storage) {
        Ok(value) => {
            if let Some(backup) = value.get("backupWorkspaces") {
                for key in ["folders", "workspaces"] {
                    if let Some(entries) = backup.get(key).and_then(Value::as_array) {
                        for entry in entries.iter().rev() {
                            add_entry(entry, &mut projects, &mut seen);
                        }
                    }
                }
            }
            if let Some(workspaces) = value
                .pointer("/profileAssociations/workspaces")
                .and_then(Value::as_object)
            {
                for uri in workspaces.keys() {
                    let kind = if uri.ends_with(".code-workspace") {
                        Kind::Workspace
                    } else {
                        Kind::Folder
                    };
                    add_uri(uri, kind, &mut projects, &mut seen);
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => errors.push(
            tr!(
                "无法读取 VS Code 项目补充信息。",
                "Could not read VS Code project metadata."
            )
            .into(),
        ),
    }
    (projects, errors.first().cloned())
}

fn read_database(path: &Path) -> Result<Option<Value>, String> {
    match fs::metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => {
            return Err(tr!(
                "无法访问 VS Code 最近项目记录。",
                "Could not access VS Code recent projects."
            )
            .into());
        }
        Ok(_) => {}
    }
    let read = || -> rusqlite::Result<Option<String>> {
        let database = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        database.busy_timeout(Duration::from_millis(250))?;
        database.query_row(
            "SELECT CAST(value AS TEXT), length(value) FROM ItemTable WHERE key IN ('recently.opened', 'history.recentlyOpenedPathsList') ORDER BY CASE key WHEN 'recently.opened' THEN 0 ELSE 1 END LIMIT 1",
            [], |row| {
                if row.get::<_, i64>(1)? > MAX_BYTES as i64 { return Err(rusqlite::Error::InvalidQuery); }
                row.get(0)
            },
        ).optional()
    };
    let value = read().map_err(|error| {
        trf!(
            "无法读取 VS Code 最近项目：{}",
            "Could not read VS Code recent projects: {}",
            error
        )
    })?;
    value
        .map(|text| {
            serde_json::from_str(&text).map_err(|_| {
                tr!("最近项目数据格式无效。", "Invalid recent-project data.").to_owned()
            })
        })
        .transpose()
}

fn read_json(path: &Path) -> std::io::Result<Value> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        return Err(std::io::Error::other("JSON exceeds size limit"));
    }
    serde_json::from_slice(&bytes).map_err(std::io::Error::other)
}

fn add_entry(entry: &Value, projects: &mut Vec<Project>, seen: &mut HashSet<PathBuf>) {
    if entry
        .get("remoteAuthority")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
    {
        return;
    }
    if let Some(uri) = entry.get("folderUri").and_then(Value::as_str) {
        add_uri(uri, Kind::Folder, projects, seen);
    } else if let Some(uri) = entry
        .pointer("/workspace/configPath")
        .and_then(Value::as_str)
    {
        add_uri(uri, Kind::Workspace, projects, seen);
    }
}

fn add_uri(uri: &str, kind: Kind, projects: &mut Vec<Project>, seen: &mut HashSet<PathBuf>) {
    if projects.len() >= MAX_PROJECTS {
        return;
    }
    let Some(path) = local_path(uri) else {
        return;
    };
    if kind == Kind::Workspace && path.extension().is_none_or(|ext| ext != "code-workspace") {
        return;
    }
    if !seen.insert(path.clone()) {
        return;
    }
    let name = match kind {
        Kind::Folder => path.file_name(),
        Kind::Workspace => path.file_stem(),
    }
    .map(|name| name.to_string_lossy().into_owned())
    .unwrap_or_else(|| "/".into());
    projects.push(Project { path, name, kind });
}

fn local_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let path = if rest.starts_with('/') {
        rest
    } else {
        rest.strip_prefix("localhost")?
    };
    if !path.starts_with('/') || path.contains(['?', '#']) {
        return None;
    }
    let bytes = path.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'%'
            && !bytes
                .get(index + 1..index + 3)
                .is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit))
        {
            return None;
        }
    }
    let decoded = percent_encoding::percent_decode_str(path)
        .decode_utf8()
        .ok()?;
    if decoded.contains('\0') {
        return None;
    }
    Some(PathBuf::from(decoded.as_ref()))
}
