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
    #[serde(skip)]
    reserved: BTreeSet<String>,
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
            reserved: BTreeSet::new(),
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
                    reserved: BTreeSet::new(),
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
        self.reserved.contains(&query.trim().to_ascii_lowercase())
            || self.resolve_window(query).is_some()
            || self.resolve(query).is_some()
    }

    pub fn filter_order(
        &self,
        query: &str,
        order: &[usize],
        windows: &[crate::core::search::WindowInfo],
    ) -> Option<Vec<usize>> {
        if !self.is_alias(query) {
            return None;
        }
        let target = self.resolve_window(query);
        Some(
            order
                .iter()
                .copied()
                .filter(|&index| {
                    if let Some(target) = target {
                        windows[index].id == target
                    } else {
                        self.reserved.contains(&query.trim().to_ascii_lowercase())
                            && self.windows.get(&windows[index].id).is_some_and(|window| {
                                Some(window.app.as_str()) == self.resolve(query)
                            })
                    }
                })
                .collect(),
        )
    }

    pub fn search_order(
        &self,
        query: &str,
        order: &[usize],
        windows: &[crate::core::search::WindowInfo],
    ) -> Option<Vec<usize>> {
        let Some(target) = self.resolve_window(query) else {
            return self.filter_order(query, order, windows);
        };
        let app = &self.windows.get(&target)?.app;
        let mut matches: Vec<_> = order
            .iter()
            .copied()
            .filter(|&index| {
                self.windows
                    .get(&windows[index].id)
                    .is_some_and(|window| &window.app == app)
            })
            .collect();
        matches.sort_by_key(|&index| windows[index].id != target);
        Some(matches)
    }

    pub fn ensure(&mut self, apps: &[AppIdentity]) -> bool {
        self.ensure_reserved(apps, &BTreeSet::new())
    }

    fn ensure_reserved(&mut self, apps: &[AppIdentity], reserved: &BTreeSet<String>) -> bool {
        let mut apps: Vec<_> = apps.iter().filter(|app| !app.id.is_empty()).collect();
        apps.sort_by_key(|app| (app.english_name.to_ascii_lowercase(), &app.id));
        let mut used: BTreeSet<_> = self
            .apps
            .values()
            .cloned()
            .chain(self.windows.values().map(|window| window.alias.clone()))
            .chain(reserved.iter().cloned())
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
        windows: &[crate::core::search::WindowInfo],
        identities: &HashMap<i32, AppIdentity>,
    ) -> bool {
        self.ensure_windows_reserved(windows, identities, &BTreeSet::new())
    }

    fn ensure_windows_reserved(
        &mut self,
        windows: &[crate::core::search::WindowInfo],
        identities: &HashMap<i32, AppIdentity>,
        reserved: &BTreeSet<String>,
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
        changed |= self.ensure_reserved(&apps, reserved);
        let mut used: BTreeSet<_> = self
            .apps
            .values()
            .cloned()
            .chain(self.windows.values().map(|window| window.alias.clone()))
            .chain(reserved.iter().cloned())
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

    pub fn with_rules(
        &self,
        windows: &[crate::core::search::WindowInfo],
        identities: &HashMap<i32, AppIdentity>,
        rules: &[crate::core::config::AliasRule],
    ) -> Self {
        if rules.is_empty() {
            return self.clone();
        }
        let mut result = self.clone();
        let reserved: BTreeSet<_> = rules.iter().map(|rule| rule.alias.clone()).collect();
        result.apps.retain(|_, alias| !reserved.contains(alias));
        result
            .windows
            .retain(|_, window| !reserved.contains(&window.alias));
        for rule in rules.iter().filter(|rule| rule.title_contains.is_empty()) {
            if let Some(application) = &rule.application {
                result
                    .apps
                    .insert(application.bundle_id.clone(), rule.alias.clone());
            }
        }
        // More specific project rules claim their windows before app-wide rules.
        let mut rules: Vec<_> = rules.iter().collect();
        rules.sort_by_key(|rule| (std::cmp::Reverse(rule.title_contains.len()), &rule.alias));
        let mut claimed = BTreeSet::new();
        for rule in rules {
            let Some(application) = &rule.application else {
                continue;
            };
            let needle = rule.title_contains.to_lowercase();
            let selected = windows
                .iter()
                .filter(|window| {
                    !claimed.contains(&window.id)
                        && identities
                            .get(&window.pid)
                            .is_some_and(|app| app.id == application.bundle_id)
                        && window.title.to_lowercase().contains(&needle)
                })
                .min_by_key(|window| window.id);
            if let Some(window) = selected {
                claimed.insert(window.id);
                result.windows.insert(
                    window.id,
                    WindowAlias {
                        app: application.bundle_id.clone(),
                        alias: rule.alias.clone(),
                    },
                );
            }
        }
        result.ensure_windows_reserved(windows, identities, &reserved);
        result.reserved = reserved;
        result
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
        if self.reserved.contains(&query)
            && let Some(app) = self.resolve(&query)
            && let Some(position) = ordered_windows
                .iter()
                .position(|id| self.windows.get(id).is_some_and(|window| window.app == app))
        {
            return AliasMatch::Matched(position);
        }
        if self.reserved.contains(&query) {
            return AliasMatch::Missing;
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
