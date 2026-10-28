//! Names the user gives to windows, so windows with similar titles can be told
//! apart in the switcher.
//!
//! A label is not attached to an accessibility identity, which apps rebuild and
//! which does not survive a restart. It is attached to the app plus the window
//! title, with the WindowServer identity as a tie-breaker for two windows that
//! share both.

use crate::{tr, trf};
use serde::{Deserialize, Serialize};

/// The longest label accepted, in characters.
pub const MAX_LABEL_CHARS: usize = 60;
/// How many labels are kept.
pub const MAX_LABELS: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowLabel {
    pub bundle_id: String,
    /// The window title as stored, normalized for comparison.
    pub title_key: String,
    /// The WindowServer identity seen when the label was given, if it had one.
    #[serde(default)]
    pub server_id: Option<u32>,
    /// What the user typed, shown instead of the title.
    pub label: String,
}

/// The window a label is compared against.
#[derive(Clone, Copy, Debug)]
pub struct WindowIdentity<'a> {
    pub bundle_id: &'a str,
    pub title: &'a str,
    pub server_id: Option<u32>,
}

impl WindowLabel {
    /// Same application and same title. The WindowServer identity is not part of
    /// this test: it changes across restarts. Two windows that share a title are
    /// told apart by the identity only when picking between their labels.
    pub fn matches(&self, window: &WindowIdentity<'_>) -> bool {
        self.bundle_id == window.bundle_id && self.title_key == normalize_title(window.title)
    }
}

/// Titles differ in spacing and case between scans, so labels compare a
/// normalized form: trips trimmed and whitespace collapsed, case-insensitive.
pub fn normalize_title(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The label for a window. An exact WindowServer identity wins over a label that
/// only matches app and title, so two windows with the same title keep their own.
pub fn label_for<'a>(
    labels: &'a [WindowLabel],
    window: &WindowIdentity<'_>,
) -> Option<&'a WindowLabel> {
    // The WindowServer identity is the strongest signal: a title can gain or lose
    // a suffix while the window itself stays the same.
    if let Some(server_id) = window.server_id
        && let Some(label) = labels
            .iter()
            .find(|label| label.bundle_id == window.bundle_id && label.server_id == Some(server_id))
    {
        return Some(label);
    }
    labels.iter().find(|label| label.matches(window))
}

/// Give this window a label, replacing whatever it had. An empty label removes
/// it, which is how renaming back to the title works.
pub fn set_label(
    labels: &mut Vec<WindowLabel>,
    window: &WindowIdentity<'_>,
    label: &str,
) -> Result<(), String> {
    let label = label.trim();
    if label.chars().count() > MAX_LABEL_CHARS {
        return Err(trf!(
            "别名不能超过 {} 个字符。",
            "A name must be at most {} characters.",
            MAX_LABEL_CHARS
        ));
    }
    // Renaming replaces the label this window had, keeping the identity it was
    // named with, so a later scan under a new identity still matches it.
    // Clearing is a request about the label this window shows, whatever identity
    // it reports now; naming a different window is not the same thing.
    if label.is_empty() {
        if let Some(existing) = label_for(labels, window).cloned() {
            labels.retain(|entry| *entry != existing);
        }
        return Ok(());
    }
    let is_same_window = |existing: &WindowLabel| {
        existing.server_id.is_none() || existing.server_id == window.server_id
    };
    if let Some(existing) = label_for(labels, window).cloned()
        && is_same_window(&existing)
    {
        labels.retain(|entry| *entry != existing);
        if !label.is_empty() {
            labels.push(WindowLabel {
                label: label.to_owned(),
                ..existing
            });
        }
        return Ok(());
    }
    if label.is_empty() {
        return Ok(());
    }
    labels.push(WindowLabel {
        bundle_id: window.bundle_id.to_owned(),
        title_key: normalize_title(window.title),
        server_id: window.server_id,
        label: label.to_owned(),
    });
    if labels.len() > MAX_LABELS {
        return Err(trf!(
            "最多保存 {} 个窗口别名。",
            "You can keep up to {} window names.",
            MAX_LABELS
        ));
    }
    Ok(())
}

/// Labels read back from disk are checked before they are used.
pub fn validate(labels: &[WindowLabel]) -> Result<(), String> {
    if labels.len() > MAX_LABELS {
        return Err(trf!(
            "最多保存 {} 个窗口别名。",
            "You can keep up to {} window names.",
            MAX_LABELS
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for label in labels {
        if label.bundle_id.trim().is_empty() || label.label.trim().is_empty() {
            return Err(tr!("保存的窗口别名不合法。", "A saved window name is invalid.").into());
        }
        if label.label.chars().count() > MAX_LABEL_CHARS {
            return Err(trf!(
                "别名不能超过 {} 个字符。",
                "A name must be at most {} characters.",
                MAX_LABEL_CHARS
            ));
        }
        if !seen.insert((
            label.bundle_id.clone(),
            label.title_key.clone(),
            label.server_id,
        )) {
            return Err(tr!("保存的窗口别名重复。", "A saved window name is duplicated.").into());
        }
    }
    Ok(())
}
