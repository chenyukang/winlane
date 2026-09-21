use crate::{core::config::ApplicationTarget, tr};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub interval_secs: u16,
    pub rules: Vec<Rule>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: 10,
            rules: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub application: ApplicationTarget,
    pub max_windows: u16,
}

impl Settings {
    pub fn interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(u64::from(self.interval_secs))
    }

    pub fn grace_period_ms(&self) -> u64 {
        u64::from(self.interval_secs) * 2_000
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(1..=3600).contains(&self.interval_secs) {
            return Err(tr!(
                "检查间隔应为 1–3600 秒的整数。",
                "Check interval must be an integer from 1 to 3600 seconds."
            )
            .into());
        }
        if self.rules.len() > 64 {
            return Err(tr!(
                "最多设置 64 条自动关闭窗口规则。",
                "You can configure up to 64 Auto AppClose rules."
            )
            .into());
        }
        let mut apps = HashSet::new();
        for rule in &self.rules {
            rule.application.validate()?;
            if !(1..=100).contains(&rule.max_windows) {
                return Err(tr!(
                    "保留窗口数应为 1–100 的整数。",
                    "Keep between 1 and 100 windows."
                )
                .into());
            }
            if rule.application.bundle_id == "app.windowlane.desktop" {
                return Err(tr!(
                    "不能自动关闭 Winlane 自己的窗口。",
                    "Winlane cannot automatically close its own windows."
                )
                .into());
            }
            if !apps.insert(&rule.application.bundle_id) {
                return Err(tr!(
                    "每个应用只能设置一条自动关闭窗口规则。",
                    "Only one Auto AppClose rule is allowed per app."
                )
                .into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    pub id: u64,
    pub pid: i32,
    pub server_id: Option<u32>,
    pub protected: bool,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub bundle_id: String,
    pub windows: Vec<Window>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub bundle_id: String,
    pub window: Window,
    pub max_windows: u16,
}

#[derive(Default)]
pub struct Planner {
    seen: HashMap<u64, u64>,
    pending: HashMap<String, Target>,
}

impl Planner {
    pub fn observe(&mut self, snapshots: &[Snapshot], now_ms: u64) {
        let live: HashSet<_> = snapshots
            .iter()
            .flat_map(|s| s.windows.iter().map(|w| w.id))
            .collect();
        self.seen.retain(|id, _| live.contains(id));
        for id in live {
            self.seen.entry(id).or_insert(now_ms);
        }
    }

    pub fn candidate(
        &self,
        settings: &Settings,
        snapshots: &[Snapshot],
        recent: &[u64],
        now_ms: u64,
    ) -> Option<Target> {
        if !settings.enabled {
            return None;
        }
        let ranks: HashMap<_, _> = recent.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        for rule in &settings.rules {
            if self.pending.contains_key(&rule.application.bundle_id) {
                continue;
            }
            let Some(snapshot) = snapshots
                .iter()
                .find(|s| s.bundle_id == rule.application.bundle_id)
            else {
                continue;
            };
            if snapshot.windows.len() <= usize::from(rule.max_windows) {
                continue;
            }
            let oldest = snapshot
                .windows
                .iter()
                .filter(|w| {
                    !w.protected
                        && w.server_id.is_some()
                        && self.seen.get(&w.id).is_some_and(|first| {
                            now_ms.saturating_sub(*first) >= settings.grace_period_ms()
                        })
                })
                .max_by_key(|w| {
                    (
                        ranks.get(&w.id).copied().unwrap_or(usize::MAX),
                        std::cmp::Reverse(w.id),
                    )
                });
            if let Some(window) = oldest {
                return Some(Target {
                    bundle_id: snapshot.bundle_id.clone(),
                    window: window.clone(),
                    max_windows: rule.max_windows,
                });
            }
        }
        None
    }

    pub fn pending(&self) -> impl Iterator<Item = &Target> {
        self.pending.values()
    }

    pub fn attempted(&mut self, target: Target) {
        // Never move on to newer windows while the app may be waiting for a save decision.
        self.pending.insert(target.bundle_id.clone(), target);
    }

    pub fn confirm_closed(&mut self, ids: &[u64]) -> Vec<Target> {
        let mut closed = Vec::new();
        self.pending.retain(|_, target| {
            if ids.contains(&target.window.id) {
                closed.push(target.clone());
                false
            } else {
                true
            }
        });
        closed
    }
}
