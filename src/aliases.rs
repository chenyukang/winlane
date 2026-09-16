use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct AppIdentity {
    pub id: String,
    pub english_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Aliases(BTreeMap<String, String>);

impl Default for Aliases {
    fn default() -> Self {
        Self(BTreeMap::from([(
            "com.tencent.xinWeChat".into(),
            "w".into(),
        )]))
    }
}

impl Aliases {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let aliases: Self = serde_json::from_str(json)
            .map_err(|_| "保存的 alias 无法读取，已保留原数据。".to_string())?;
        let mut used = BTreeSet::new();
        for (app, alias) in &aliases.0 {
            if app.is_empty()
                || !(1..=2).contains(&alias.len())
                || !alias.bytes().all(|ch| ch.is_ascii_lowercase())
                || !used.insert(alias)
            {
                return Err("保存的 alias 不合法或重复，已保留原数据。".into());
            }
        }
        Ok(aliases)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("string maps serialize without failure")
    }

    pub fn get(&self, app: &str) -> Option<&str> {
        self.0.get(app).map(String::as_str)
    }

    pub fn resolve(&self, query: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(_, alias)| alias.eq_ignore_ascii_case(query.trim()))
            .map(|(app, _)| app.as_str())
    }

    pub fn has_prefix(&self, query: &str) -> bool {
        self.0.values().any(|alias| alias.starts_with(query))
    }

    pub fn filter_order(
        &self,
        query: &str,
        order: &[usize],
        windows: &[crate::search::WindowInfo],
        identities: &HashMap<i32, AppIdentity>,
    ) -> Option<Vec<usize>> {
        let target = self.resolve(query)?;
        Some(
            order
                .iter()
                .copied()
                .filter(|&index| {
                    identities
                        .get(&windows[index].pid)
                        .is_some_and(|app| app.id == target)
                })
                .collect(),
        )
    }

    pub fn ensure(&mut self, apps: &[AppIdentity]) -> bool {
        let mut apps: Vec<_> = apps.iter().filter(|app| !app.id.is_empty()).collect();
        apps.sort_by_key(|app| (app.english_name.to_ascii_lowercase(), &app.id));
        let mut used: BTreeSet<_> = self.0.values().cloned().collect();
        let mut changed = false;
        for app in apps {
            if self.0.contains_key(&app.id) {
                continue;
            }
            if let Some(alias) = candidates(&app.english_name)
                .into_iter()
                .find(|alias| !used.contains(alias))
            {
                used.insert(alias.clone());
                self.0.insert(app.id.clone(), alias);
                changed = true;
            }
        }
        changed
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

pub fn matching_alias_position(
    aliases: &Aliases,
    query: &str,
    ordered_apps: &[&str],
) -> Option<usize> {
    let app = aliases.resolve(query)?;
    ordered_apps.iter().position(|id| *id == app)
}
