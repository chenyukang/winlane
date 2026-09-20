use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

use crate::core::config::ApplicationTarget;
use crate::core::input_method::InputMethod;

use crate::{tr, trf};

pub const WINLANE_ID: &str = "app.windowlane.desktop";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputSource {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RestoreStrategy {
    #[default]
    Default,
    LastUsed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceRule {
    #[default]
    Global,
    Current,
    English,
    Chinese,
    Source(InputSource),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRule {
    pub application: ApplicationTarget,
    #[serde(default)]
    pub source: SourceRule,
    #[serde(default)]
    pub restore: Option<RestoreStrategy>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub default_source: SourceRule,
    pub restore: RestoreStrategy,
    pub apps: Vec<AppRule>,
}

impl Default for Settings {
    fn default() -> Self {
        Self::for_winlane(InputMethod::English)
    }
}

impl Settings {
    pub fn for_winlane(policy: InputMethod) -> Self {
        let (source, restore) = match policy {
            InputMethod::Current => (SourceRule::Current, RestoreStrategy::Default),
            InputMethod::English => (SourceRule::English, RestoreStrategy::Default),
            InputMethod::Chinese => (SourceRule::Chinese, RestoreStrategy::Default),
            InputMethod::LastUsed => (SourceRule::Current, RestoreStrategy::LastUsed),
        };
        Self {
            enabled: false,
            default_source: SourceRule::Current,
            restore: RestoreStrategy::Default,
            apps: vec![AppRule {
                application: ApplicationTarget {
                    bundle_id: WINLANE_ID.into(),
                    name: "Winlane".into(),
                    path: "/Applications/Winlane.app".into(),
                },
                source,
                restore: Some(restore),
            }],
        }
    }

    // Session behavior keeps the existing restoration and early-input machinery.
    // The exact target source is resolved from the rule, not from this category.
    pub fn winlane_policy(&self) -> InputMethod {
        let (source, restore) = self.resolve(WINLANE_ID);
        if restore == RestoreStrategy::LastUsed {
            return InputMethod::LastUsed;
        }
        match source {
            SourceRule::English => InputMethod::English,
            SourceRule::Chinese => InputMethod::Chinese,
            SourceRule::Source(_) => InputMethod::LastUsed,
            _ => InputMethod::Current,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.apps.len() > 64 {
            return Err(tr!(
                "最多设置 64 条输入法规则。",
                "You can configure up to 64 input source rules."
            )
            .into());
        }
        let mut apps = HashSet::new();
        for rule in &self.apps {
            rule.application.validate()?;
            if !apps.insert(&rule.application.bundle_id) {
                return Err(trf!(
                    "{} 已有输入法规则。",
                    "{} already has an input source rule.",
                    rule.application.name
                ));
            }
        }
        if self.default_source == SourceRule::Global || !apps.contains(&WINLANE_ID.to_owned()) {
            return Err(tr!(
                "输入法设置需要有效的全局规则和 Winlane 规则。",
                "Input settings need a valid global rule and a Winlane rule."
            )
            .into());
        }
        for rule in
            std::iter::once(&self.default_source).chain(self.apps.iter().map(|rule| &rule.source))
        {
            let SourceRule::Source(source) = rule else {
                continue;
            };
            if !valid_id(&source.id) || source.name.trim().is_empty() || source.name.len() > 512 {
                return Err(tr!("请选择有效的输入法。", "Choose a valid input source.").into());
            }
        }
        Ok(())
    }

    pub fn resolve(&self, app: &str) -> (&SourceRule, RestoreStrategy) {
        let rule = self
            .apps
            .iter()
            .find(|rule| rule.application.bundle_id == app);
        let source = match rule.map(|rule| &rule.source) {
            None | Some(SourceRule::Global) => &self.default_source,
            Some(source) => source,
        };
        (
            source,
            rule.and_then(|rule| rule.restore).unwrap_or(self.restore),
        )
    }
}

fn valid_id(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= 512 && !id.chars().any(char::is_control)
}

/// App activation chooses a source once. Source-change events only remember it;
/// they never enforce a rule while the user is typing.
#[derive(Default)]
pub struct Runtime {
    active: Option<(String, i32)>,
    remembered: BTreeMap<String, String>,
}

impl Runtime {
    pub fn reset_activation(&mut self) {
        self.active = None;
    }

    pub fn activate(
        &mut self,
        settings: &Settings,
        app: Option<(&str, i32)>,
        current: Option<&str>,
        resolve: impl Fn(&SourceRule) -> Option<String>,
    ) -> Option<String> {
        let app = app.filter(|(id, _)| *id != "app.windowlane.desktop");
        let next = app.map(|(id, pid)| (id.to_owned(), pid));
        if next == self.active {
            return None;
        }
        self.active = next;
        let (id, pid) = app?;
        if !settings.enabled {
            return None;
        }
        let (default, restore) = settings.resolve(id);
        let remembered = (restore == RestoreStrategy::LastUsed)
            .then(|| self.remembered.get(id).map(String::as_str))
            .flatten();
        let target = remembered
            .and_then(|id| {
                resolve(&SourceRule::Source(InputSource {
                    id: id.to_owned(),
                    name: id.to_owned(),
                }))
            })
            .or_else(|| resolve(default));
        if target
            .as_deref()
            .is_none_or(|target| Some(target) == current)
        {
            if let Some(current) = current {
                self.observe(settings, id, pid, current);
            }
            return None;
        }
        target
    }

    pub fn observe(&mut self, settings: &Settings, app: &str, pid: i32, source: &str) {
        if settings.enabled
            && valid_id(source)
            && self
                .active
                .as_ref()
                .is_some_and(|(id, active_pid)| id == app && *active_pid == pid)
        {
            // Keep the cache bounded without discarding existing apps' history.
            if self.remembered.len() < 512 || self.remembered.contains_key(app) {
                self.remembered.insert(app.to_owned(), source.to_owned());
            }
        }
    }

    pub fn restore_history(&mut self, text: &str) {
        if let Ok(history) = serde_json::from_str::<BTreeMap<String, String>>(text) {
            self.remembered = history
                .into_iter()
                .filter(|(app, source)| {
                    valid_id(app) && valid_id(source) && app != "app.windowlane.desktop"
                })
                .take(512)
                .collect();
        }
    }

    pub fn history(&self) -> String {
        serde_json::to_string(&self.remembered).expect("string map is serializable")
    }
}
