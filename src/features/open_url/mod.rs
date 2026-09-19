//! URL navigation, Google search and on-demand Chrome history.
mod input;
mod snapshot;
pub use input::{InputTarget, input_target};

use crate::{tr, trf};
use rusqlite::{Connection, OpenFlags};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const MAX_URLS: usize = 1000;
pub const MAX_RESULTS: usize = 100;
const MAX_PROFILES: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub url: String,
    pub title: String,
    pub last_visit_time: i64,
}

#[derive(Default)]
pub struct History {
    pub pages: Vec<Page>,
    pub error: Option<String>,
    pub access_denied: bool,
}

impl History {
    fn record_access_error(&mut self, error: &(dyn std::error::Error + 'static)) {
        self.access_denied |= error
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::PermissionDenied);
        if self.access_denied {
            self.error = Some(
                tr!(
                    "macOS 阻止了 Chrome 历史访问。请为 Winlane 授权后重启。",
                    "macOS blocked Chrome history access. Allow Winlane access, then restart it."
                )
                .into(),
            );
        }
    }
}

pub fn chrome_directory(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Google/Chrome")
}

pub fn matching(pages: &[Page], query: &str) -> Vec<Page> {
    let terms: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut matches: Vec<_> = pages
        .iter()
        .filter_map(|page| {
            if terms.is_empty() {
                return Some(((0, 0), page));
            }
            let title = page.title.to_lowercase();
            let url = page.url.to_lowercase();
            if !terms
                .iter()
                .all(|term| title.contains(term) || url.contains(term))
            {
                return None;
            }
            let parsed = url::Url::parse(&page.url).ok();
            let host = parsed.as_ref().and_then(|url| url.host_str()).unwrap_or("");
            let host = host.strip_prefix("www.").unwrap_or(host);
            let rank = terms.iter().fold((0, 0), |(worst, total), term| {
                let term = term.strip_prefix("www.").unwrap_or(term);
                let rank = if host == term {
                    0
                } else if host.starts_with(term) {
                    1
                } else if host.contains(term) {
                    2
                } else {
                    3
                };
                (worst.max(rank), total + rank)
            });
            Some((rank, page))
        })
        .collect();
    matches.sort_by(|(a_rank, a), (b_rank, b)| {
        a_rank
            .cmp(b_rank)
            .then_with(|| b.last_visit_time.cmp(&a.last_visit_time))
    });
    matches
        .into_iter()
        .take(MAX_RESULTS)
        .map(|(_, page)| page.clone())
        .collect()
}

pub fn is_web_url(url: &str) -> bool {
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };
    !url.chars().any(|ch| ch.is_control() || ch.is_whitespace())
        && !rest
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default()
            .is_empty()
}

pub fn load(root: &Path) -> History {
    let mut history = History::default();
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return history,
        Err(error) => {
            history.error = Some(
                tr!(
                    "无法读取 Chrome 历史目录，按 ⌘R 重试。",
                    "Could not read the Chrome history directory. Press ⌘R to retry."
                )
                .into(),
            );
            history.record_access_error(&error);
            return history;
        }
    };
    let mut profiles = Vec::new();
    let mut failures = 0;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                failures += 1;
                history.record_access_error(&error);
                continue;
            }
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name != "Default"
            && !name
                .strip_prefix("Profile ")
                .is_some_and(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
        {
            continue;
        }
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => profiles.push(entry.path()),
            Err(error) => {
                failures += 1;
                history.record_access_error(&error);
            }
            _ => {}
        }
    }
    profiles.sort();
    if profiles.len() > MAX_PROFILES {
        failures += profiles.len() - MAX_PROFILES;
        profiles.truncate(MAX_PROFILES);
    }
    for profile in profiles {
        match fs::symlink_metadata(profile.join("History")) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                failures += 1;
                history.record_access_error(&error);
                continue;
            }
            Ok(_) => {}
        }
        match read_profile(&profile) {
            Ok(pages) => history.pages.extend(pages),
            Err(error) => {
                failures += 1;
                history.record_access_error(error.as_ref());
            }
        }
    }
    history.pages.sort_by(|a, b| {
        b.last_visit_time
            .cmp(&a.last_visit_time)
            .then_with(|| a.url.cmp(&b.url))
    });
    let mut seen = HashSet::new();
    history.pages.retain(|page| seen.insert(page.url.clone()));
    history.pages.truncate(MAX_URLS);
    if failures > 0 && !history.access_denied {
        history.error = Some(trf!(
            "有 {} 个 Chrome 配置未能读取，按 ⌘R 重试。",
            "Could not read {} Chrome profiles. Press ⌘R to retry.",
            failures
        ));
    }
    history
}

fn read_profile(profile: &Path) -> Result<Vec<Page>, Box<dyn std::error::Error>> {
    let snapshot = snapshot::Snapshot::new(profile)?;
    // Writes, if needed for journal recovery, touch only the disposable copy.
    let db = Connection::open_with_flags(
        snapshot.database(),
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA temp_store=MEMORY; PRAGMA query_only=ON;")?;
    let mut statement = db.prepare("SELECT url, substr(COALESCE(title, ''), 1, 1024), last_visit_time FROM urls WHERE hidden=0 AND last_visit_time>0 AND length(url)<=16384 AND (url LIKE 'https://%' OR url LIKE 'http://%') ORDER BY last_visit_time DESC, id DESC LIMIT ?1")?;
    let rows = statement.query_map([MAX_URLS as i64], |row| {
        let url: String = row.get(0)?;
        let title: String = row.get(1)?;
        Ok(Page {
            title: if title.trim().is_empty() {
                url.clone()
            } else {
                title
            },
            url,
            last_visit_time: row.get(2)?,
        })
    })?;
    let mut pages = Vec::new();
    for row in rows {
        let page = row?;
        if is_web_url(&page.url) {
            pages.push(page);
        }
    }
    Ok(pages)
}
