use std::collections::HashSet;
use std::path::PathBuf;

pub const MAX_RESULTS: usize = 200;

/// A local git branch offered for switching.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub alias: String,
    // Relative commit date of the branch tip, e.g. "2 hours ago".
    pub detail: String,
    // The branch currently checked out in the target repository.
    pub current: bool,
    // Checked out in a different worktree, so a plain switch would fail.
    pub elsewhere: bool,
}

/// Branches for the resolved repository, plus any read error.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Branches {
    pub items: Vec<Branch>,
    pub error: Option<String>,
    // Repository root, used both for display and to run the checkout.
    pub repo: Option<PathBuf>,
}

impl Branches {
    /// The repository folder name shown in the footer.
    pub fn repo_label(&self) -> Option<String> {
        self.repo
            .as_deref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
    }
}

/// Filter branches by a case-insensitive substring of the branch name or its alias.
pub fn matching(items: &[Branch], query: &str) -> Vec<Branch> {
    let needle = query.trim().to_lowercase();
    items
        .iter()
        .filter(|branch| {
            needle.is_empty()
                || branch.name.to_lowercase().contains(&needle)
                || alias(&branch.name).contains(&needle)
        })
        .take(MAX_RESULTS)
        .cloned()
        .collect()
}

/// A short alias derived from the branch name, e.g. `feature/search` -> `fs`.
pub fn alias(name: &str) -> String {
    let mut result = String::new();
    let mut previous = None::<char>;
    for ch in name.chars() {
        let boundary = ch.is_ascii_alphabetic()
            && previous.is_none_or(|prev| {
                !prev.is_ascii_alphabetic()
                    || matches!(prev, '/' | '-' | '_' | '.' | ' ')
                    || (prev.is_lowercase() && ch.is_uppercase())
            });
        if boundary {
            result.push(ch.to_ascii_lowercase());
            if result.len() == 2 {
                return result;
            }
        }
        previous = Some(ch);
    }
    if result.is_empty() {
        name.chars()
            .find(|ch| ch.is_ascii_alphabetic())
            .map(|ch| ch.to_ascii_lowercase().to_string())
            .unwrap_or_else(|| "?".into())
    } else {
        result
    }
}

/// Build the ordered branch list from `git for-each-ref` and `git reflog` output.
///
/// `for_each_ref` lines are tab-separated: name, relative date, HEAD marker
/// ("*" for the current branch), and the worktree path (non-empty when the
/// branch is checked out elsewhere). Branches recently visited in the reflog
/// come first, in most-recent order, then the rest by commit date.
pub fn build_branches(for_each_ref: &str, reflog: &str) -> Vec<Branch> {
    let refs: Vec<Branch> = for_each_ref
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let name = parts.next()?.trim();
            if name.is_empty() {
                return None;
            }
            let detail = parts.next().unwrap_or("").trim().to_string();
            let head = parts.next().unwrap_or("").trim();
            let worktree = parts.next().unwrap_or("").trim();
            let current = head == "*";
            Some(Branch {
                name: name.to_string(),
                alias: String::new(),
                detail,
                current,
                elsewhere: !current && !worktree.is_empty(),
            })
        })
        .collect();

    let mut seen = HashSet::new();
    let mut ordered = Vec::new();
    for target in reflog_targets(reflog) {
        if let Some(branch) = refs.iter().find(|branch| branch.name == target)
            && seen.insert(branch.name.clone())
        {
            ordered.push(branch.clone());
        }
    }
    for branch in &refs {
        if seen.insert(branch.name.clone()) {
            ordered.push(branch.clone());
        }
    }
    ordered.truncate(MAX_RESULTS);
    assign_aliases(ordered)
}

/// Branch names in reflog checkout order (most recent first), including duplicates.
fn reflog_targets(reflog: &str) -> Vec<String> {
    reflog
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("checkout: moving from ")?;
            // The destination is whatever follows the final " to ".
            rest.rsplit(" to ")
                .next()
                .map(|name| name.trim().to_string())
        })
        .filter(|name| !name.is_empty())
        .collect()
}

fn alias_candidates(name: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let base = alias(name);
    if !base.is_empty() {
        candidates.push(base);
    }
    let letters: String = name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect();
    if letters.is_empty() {
        candidates.push("?".into());
    } else {
        for length in 1..=letters.len() {
            candidates.push(letters[..length].to_string());
        }
    }
    candidates.dedup();
    candidates
}

fn assign_aliases(branches: Vec<Branch>) -> Vec<Branch> {
    let mut used = HashSet::new();
    branches
        .into_iter()
        .map(|mut branch| {
            let alias = alias_candidates(&branch.name)
                .into_iter()
                .find(|candidate| used.insert(candidate.clone()))
                .unwrap_or_else(|| "?".into());
            branch.alias = alias;
            branch
        })
        .collect()
}
