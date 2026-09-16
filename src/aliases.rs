use crate::tr;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct AppIdentity {
    pub id: String,
    pub english_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Aliases {
    apps: BTreeMap<String, String>,
    windows: BTreeMap<u64, WindowAlias>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowAlias {
    app: String,
    alias: String,
}

impl Default for Aliases {
    fn default() -> Self {
        Self {
            apps: BTreeMap::from([("com.tencent.xinWeChat".into(), "w".into())]),
            windows: BTreeMap::new(),
        }
    }
}

impl Aliases {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let aliases: Self = serde_json::from_str(json)
            .or_else(|_| {
                serde_json::from_str(json).map(|apps| Self {
                    apps,
                    windows: BTreeMap::new(),
                })
            })
            .map_err(|_| {
                tr!(
                    "保存的 alias 无法读取，已保留原数据。",
                    "Saved aliases could not be read. The original data has been preserved."
                )
                .to_string()
            })?;
        let mut used = BTreeSet::new();
        for (app, alias) in &aliases.apps {
            if app.is_empty()
                || !(1..=2).contains(&alias.len())
                || !alias.bytes().all(|ch| ch.is_ascii_lowercase())
                || !used.insert(alias)
            {
                return Err(tr!(
                    "保存的 alias 不合法或重复，已保留原数据。",
                    "Saved aliases are invalid or duplicated. The original data has been preserved."
                )
                .into());
            }
        }
        let mut window_aliases = BTreeSet::new();
        for window in aliases.windows.values() {
            if !aliases.apps.contains_key(&window.app)
                || !(1..=2).contains(&window.alias.len())
                || !window.alias.bytes().all(|ch| ch.is_ascii_lowercase())
                || !window_aliases.insert(&window.alias)
                || aliases
                    .resolve(&window.alias)
                    .is_some_and(|app| app != window.app)
            {
                return Err(tr!("保存的窗口 alias 不合法或重复，已保留原数据。", "Saved window aliases are invalid or duplicated. The original data has been preserved.").into());
            }
        }
        Ok(aliases)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("string maps serialize without failure")
    }

    pub fn get(&self, app: &str) -> Option<&str> {
        self.apps.get(app).map(String::as_str)
    }

    pub fn resolve(&self, query: &str) -> Option<&str> {
        self.apps
            .iter()
            .find(|(_, alias)| alias.eq_ignore_ascii_case(query.trim()))
            .map(|(app, _)| app.as_str())
    }

    pub fn for_window(&self, id: u64) -> Option<&str> {
        self.windows.get(&id).map(|window| window.alias.as_str())
    }

    pub fn resolve_window(&self, query: &str) -> Option<u64> {
        self.windows
            .iter()
            .find(|(_, window)| window.alias.eq_ignore_ascii_case(query.trim()))
            .map(|(&id, _)| id)
    }

    pub fn is_alias(&self, query: &str) -> bool {
        self.resolve_window(query).is_some() || self.resolve(query).is_some()
    }

    pub fn filter_order(
        &self,
        query: &str,
        order: &[usize],
        windows: &[crate::search::WindowInfo],
    ) -> Option<Vec<usize>> {
        if !self.is_alias(query) {
            return None;
        }
        let target = self.resolve_window(query);
        Some(
            order
                .iter()
                .copied()
                .filter(|&index| Some(windows[index].id) == target)
                .collect(),
        )
    }

    pub fn ensure(&mut self, apps: &[AppIdentity]) -> bool {
        let mut apps: Vec<_> = apps.iter().filter(|app| !app.id.is_empty()).collect();
        apps.sort_by_key(|app| (app.english_name.to_ascii_lowercase(), &app.id));
        let mut used: BTreeSet<_> = self
            .apps
            .values()
            .cloned()
            .chain(self.windows.values().map(|window| window.alias.clone()))
            .collect();
        let mut changed = false;
        for app in apps {
            if self.apps.contains_key(&app.id) {
                continue;
            }
            if let Some(alias) = candidates(&app.english_name)
                .into_iter()
                .find(|alias| !used.contains(alias))
            {
                used.insert(alias.clone());
                self.apps.insert(app.id.clone(), alias);
                changed = true;
            }
        }
        changed
    }

    pub fn ensure_windows(
        &mut self,
        windows: &[crate::search::WindowInfo],
        identities: &HashMap<i32, AppIdentity>,
    ) -> bool {
        let mut ordered: Vec<_> = windows
            .iter()
            .filter_map(|window| {
                identities
                    .get(&window.pid)
                    .filter(|app| !app.id.is_empty())
                    .map(|app| (window, app))
            })
            .collect();
        ordered.sort_by_key(|(window, app)| (&app.id, window.id));
        let live: BTreeMap<_, _> = ordered
            .iter()
            .map(|(window, app)| (window.id, &app.id))
            .collect();
        let before = self.windows.len();
        self.windows
            .retain(|id, saved| live.get(id).is_some_and(|app| **app == saved.app));
        let mut changed = self.windows.len() != before;
        let apps: Vec<_> = ordered.iter().map(|(_, app)| (*app).clone()).collect();
        changed |= self.ensure(&apps);
        let mut used: BTreeSet<_> = self
            .apps
            .values()
            .cloned()
            .chain(self.windows.values().map(|window| window.alias.clone()))
            .collect();
        for (window, app) in ordered {
            if self.windows.contains_key(&window.id) {
                continue;
            }
            let Some(base) = self.get(&app.id) else {
                continue;
            };
            let alias = if !self.windows.values().any(|window| window.alias == base) {
                Some(base.to_owned())
            } else {
                candidates(&app.english_name)
                    .into_iter()
                    .find(|alias| !used.contains(alias))
            };
            if let Some(alias) = alias {
                used.insert(alias.clone());
                self.windows.insert(
                    window.id,
                    WindowAlias {
                        app: app.id.clone(),
                        alias,
                    },
                );
                changed = true;
            }
        }
        changed
    }

    pub fn match_windows(&self, query: &str, ordered_windows: &[u64]) -> AliasMatch {
        let query = query.trim().to_ascii_lowercase();
        if query.is_empty() {
            return AliasMatch::Missing;
        }
        if let Some(id) = self.resolve_window(&query)
            && let Some(position) = ordered_windows.iter().position(|window| *window == id)
        {
            return AliasMatch::Matched(position);
        }
        let mut candidate = None;
        for (position, &id) in ordered_windows.iter().enumerate() {
            if self
                .for_window(id)
                .is_some_and(|alias| alias.starts_with(&query))
            {
                if candidate.is_some() {
                    return AliasMatch::Ambiguous;
                }
                candidate = Some(position);
            }
        }
        candidate.map_or(AliasMatch::Missing, AliasMatch::Matched)
    }
}

fn candidates(name: &str) -> Vec<String> {
    let letters: Vec<_> = name
        .chars()
        .filter(char::is_ascii_alphabetic)
        .map(|ch| ch.to_ascii_lowercase())
        .collect();
    let Some(&first) = letters.first() else {
        return Vec::new();
    };
    let mut result = vec![first.to_string()];
    let mut previous = None::<char>;
    let initials: Vec<_> = name
        .chars()
        .filter_map(|ch| {
            let initial = ch.is_ascii_alphabetic()
                && previous.is_none_or(|prev| {
                    !prev.is_ascii_alphabetic() || (prev.is_lowercase() && ch.is_uppercase())
                });
            previous = Some(ch);
            initial.then(|| ch.to_ascii_lowercase())
        })
        .collect();
    if initials.len() > 1 {
        result.push(format!("{}{}", first, initials[1]));
    }
    for second in letters.iter().skip(1).copied().chain('a'..='z') {
        result.push(format!("{first}{second}"));
    }
    for letter in letters.into_iter().chain('a'..='z') {
        result.push(letter.to_string());
    }
    for a in 'a'..='z' {
        for b in 'a'..='z' {
            result.push(format!("{a}{b}"));
        }
    }
    result
}

#[derive(Default, Debug)]
pub struct AliasInput(String);

impl AliasInput {
    pub fn text(&self) -> &str {
        &self.0
    }

    pub fn push(&mut self, ch: char) {
        if !ch.is_ascii_lowercase() {
            return;
        }
        if self.0.len() == 2 {
            self.0.clear();
        }
        self.0.push(ch);
    }

    pub fn pop(&mut self) {
        self.0.pop();
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AliasMatch {
    Matched(usize),
    Ambiguous,
    Missing,
}

impl AliasMatch {
    pub fn position(self) -> Option<usize> {
        match self {
            Self::Matched(position) => Some(position),
            Self::Ambiguous | Self::Missing => None,
        }
    }
}
