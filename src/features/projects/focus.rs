use super::{Kind, Project};

pub struct Window {
    pub id: u64,
    pub title: String,
    pub document: Option<String>,
}

pub fn target_window(project: &Project, windows: &[Window]) -> Option<u64> {
    let matching: Vec<_> = windows
        .iter()
        .filter_map(|window| {
            let document = window
                .document
                .as_deref()
                .and_then(|value| url::Url::parse(value).ok())
                .and_then(|url| url.to_file_path().ok());
            let path_match = document.as_deref().is_some_and(|path| match project.kind {
                Kind::Folder => path.starts_with(&project.path),
                Kind::Workspace => path == project.path,
            });
            if document.as_deref() == Some(project.path.as_path()) {
                return Some((0, window.id));
            }
            if project.kind == Kind::Folder && document.is_some() && !path_match {
                return None;
            }
            let title = window.title.trim().trim_start_matches('●').trim();
            let title = title
                .strip_suffix(" — Visual Studio Code")
                .or_else(|| title.strip_suffix(" - Visual Studio Code"))
                .unwrap_or(title);
            let name = match project.kind {
                Kind::Folder => project.path.file_name(),
                Kind::Workspace => project.path.file_stem(),
            }
            .and_then(|name| name.to_str())
            .unwrap_or(&project.name);
            let named = |name: &str| {
                !name.is_empty()
                    && (title == name
                        || title.ends_with(&format!(" — {name}"))
                        || title.ends_with(&format!(" - {name}")))
            };
            (named(name)
                || named(&project.path.to_string_lossy())
                || (project.kind == Kind::Workspace && named(&format!("{name} (Workspace)"))))
            .then_some((if path_match { 0 } else { 1 }, window.id))
        })
        .collect();
    let best = matching.iter().map(|(rank, _)| rank).min()?;
    let mut candidates = matching.iter().filter(|(rank, _)| rank == best);
    let id = candidates.next()?.1;
    candidates.next().is_none().then_some(id)
}

#[derive(Default)]
pub struct ReadyWindow(Option<u64>);

impl ReadyWindow {
    pub fn observe(&mut self, candidate: Option<u64>) -> Option<u64> {
        let ready = candidate.filter(|id| Some(*id) == self.0);
        self.0 = candidate;
        ready
    }
}

pub fn is_origin_or_target(origin: i32, front: i32, target: bool, target_seen: &mut bool) -> bool {
    if target {
        *target_seen = true;
        true
    } else {
        front == origin && !*target_seen
    }
}
