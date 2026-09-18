use crate::{tr, trf};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const MAX_ITEM_BYTES: usize = 64 * 1024;
pub const MAX_HISTORY_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClipboardSettings {
    pub enabled: bool,
    pub persistent: bool,
    pub max_items: usize,
    pub retention_days: u16,
}
impl Default for ClipboardSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            persistent: true,
            max_items: 200,
            retention_days: 7,
        }
    }
}
impl ClipboardSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=1000).contains(&self.max_items) || !(1..=365).contains(&self.retention_days) {
            return Err(tr!(
                "剪贴板历史上限为 1–1000 条，保留时间为 1–365 天。",
                "Clipboard history allows 1–1000 entries and 1–365 days."
            )
            .into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    pub text: Arc<str>,
    pub copied_at: u64,
    pub source: String,
}
impl Entry {
    pub fn preview(&self) -> String {
        let mut result = String::new();
        let mut whitespace = false;
        let mut length = 0;
        for ch in self.text.chars() {
            if ch.is_whitespace() {
                whitespace = !result.is_empty();
                continue;
            }
            if whitespace {
                result.push(' ');
                length += 1;
                whitespace = false;
            }
            result.push(ch);
            length += 1;
            if length >= 180 {
                break;
            }
        }
        result
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct History {
    pub entries: Vec<Entry>,
}
impl History {
    pub fn record(
        &mut self,
        text: &str,
        source: &str,
        now: u64,
        settings: &ClipboardSettings,
    ) -> bool {
        if !settings.enabled || text.trim().is_empty() || text.len() > MAX_ITEM_BYTES {
            return false;
        }
        let existing = self
            .entries
            .iter()
            .find(|entry| &*entry.text == text)
            .map(|entry| entry.id);
        let id = existing.unwrap_or_else(|| {
            self.entries
                .iter()
                .map(|entry| entry.id)
                .max()
                .unwrap_or(0)
                .saturating_add(1)
        });
        self.entries.retain(|entry| &*entry.text != text);
        self.entries.insert(
            0,
            Entry {
                id,
                text: Arc::from(text),
                copied_at: now,
                source: source.chars().take(100).collect(),
            },
        );
        self.prune(now, settings);
        true
    }
    pub fn prune(&mut self, now: u64, settings: &ClipboardSettings) -> bool {
        let before = self.entries.len();
        let cutoff = now.saturating_sub(u64::from(settings.retention_days) * 86400);
        let mut bytes = 0usize;
        let mut count = 0usize;
        self.entries.retain(|entry| {
            if entry.copied_at < cutoff
                || count >= settings.max_items
                || bytes.saturating_add(entry.text.len()) > MAX_HISTORY_BYTES
            {
                return false;
            }
            bytes += entry.text.len();
            count += 1;
            true
        });
        before != self.entries.len()
    }
    pub fn get(&self, id: u64) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        before != self.entries.len()
    }
    pub fn matching(&self, query: &str) -> Vec<u64> {
        let query = query.trim().to_lowercase();
        let terms: Vec<_> = query.split_whitespace().collect();
        self.entries
            .iter()
            .filter(|entry| {
                if terms.is_empty() {
                    return true;
                }
                let text = entry.text.to_lowercase();
                let source = entry.source.to_lowercase();
                terms
                    .iter()
                    .all(|term| text.contains(term) || source.contains(term))
            })
            .map(|entry| entry.id)
            .collect()
    }
    pub fn from_json(bytes: &[u8], now: u64, settings: &ClipboardSettings) -> Result<Self, String> {
        let mut history: Self = serde_json::from_slice(bytes).map_err(|_| {
            tr!(
                "剪贴板历史无法读取，原文件已保留。",
                "Clipboard history could not be read. The original file was preserved."
            )
        })?;
        let mut ids = std::collections::HashSet::new();
        let mut texts = std::collections::HashSet::new();
        if history.entries.len() > 1000
            || history.entries.iter().any(|entry| {
                entry.id == 0
                    || entry.id == u64::MAX
                    || entry.text.trim().is_empty()
                    || entry.text.len() > MAX_ITEM_BYTES
                    || entry.source.chars().count() > 100
                    || !ids.insert(entry.id)
                    || !texts.insert(entry.text.clone())
            })
        {
            return Err(tr!(
                "剪贴板历史文件无效，原文件已保留。",
                "Invalid clipboard history file. The original file was preserved."
            )
            .into());
        }
        history
            .entries
            .sort_by_key(|entry| std::cmp::Reverse(entry.copied_at));
        history.prune(now, settings);
        Ok(history)
    }
}

pub fn ignored(types: &[String], bundle_id: &str) -> bool {
    types.iter().any(|kind| {
        matches!(
            kind.as_str(),
            "org.nspasteboard.ConcealedType"
                | "org.nspasteboard.TransientType"
                | "org.nspasteboard.AutoGeneratedType"
                | "com.agilebits.onepassword"
                | "de.petermaurer.TransientPasteboardType"
                | "Pasteboard generator type"
                | "public.file-url"
                | "public.png"
                | "public.tiff"
        )
    }) || matches!(
        bundle_id,
        "app.windowlane.desktop"
            | "com.apple.Passwords"
            | "com.apple.keychainaccess"
            | "com.agilebits.onepassword7"
            | "com.1password.1password"
            | "com.bitwarden.desktop"
            | "org.keepassxc.keepassxc"
            | "com.lastpass.LastPass"
            | "com.dashlane.Dashlane"
    )
}

pub fn entry_count(count: usize, paused: bool) -> String {
    if paused {
        trf!(
            "{} 条历史 · 已暂停记录",
            "{} entries · Recording paused",
            count
        )
    } else {
        trf!("{} 条历史", "{} entries", count)
    }
}
