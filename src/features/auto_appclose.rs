use crate::{core::config::ApplicationTarget, tr, trf};
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

/// The idle time offered when a rule is suggested, for example for an app that
/// is keeping the Mac awake.
pub const SUGGESTED_IDLE_MINUTES: u16 = 60;

/// The longest idle time a rule accepts, one week. Idle closing works in
/// minutes because hours and days are the usual choices.
pub const MAX_IDLE_MINUTES: u16 = 10_080;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub application: ApplicationTarget,
    /// Close the least recently used windows while an app keeps more than this
    /// many. `None` leaves the number of windows alone.
    #[serde(default)]
    pub max_windows: Option<u16>,
    /// Close a window that has not been used for this long. `None` never closes
    /// a window for being idle.
    #[serde(default)]
    pub max_idle_minutes: Option<u16>,
}

impl Rule {
    /// Whether the rule asks for anything. A rule with both limits empty is a
    /// draft that is kept in the settings but never checks or closes a window.
    pub fn limits_anything(&self) -> bool {
        self.max_windows.is_some() || self.max_idle_minutes.is_some()
    }
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
            if rule
                .max_windows
                .is_some_and(|max| !(1..=100).contains(&max))
            {
                return Err(tr!(
                    "保留窗口数应为 1–100 的整数，留空表示不限制。",
                    "Keep between 1 and 100 windows, or leave it empty for no limit."
                )
                .into());
            }
            if rule
                .max_idle_minutes
                .is_some_and(|minutes| !(1..=MAX_IDLE_MINUTES).contains(&minutes))
            {
                return Err(trf!(
                    "闲置时间应为 1–{MAX_IDLE_MINUTES} 分钟的整数，留空表示不按闲置关闭。",
                    "Idle time must be an integer from 1 to {MAX_IDLE_MINUTES} minutes, or empty to never close for being idle."
                ));
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
    /// The window the user is looking at, when this app is frontmost. It counts
    /// as used right now, which no close may overtake.
    pub focused: Option<u64>,
}

/// Which limit asked for a close, for the log and for re-validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    WindowLimit,
    Idle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub bundle_id: String,
    pub window: Window,
    pub reason: Reason,
    pub max_windows: Option<u16>,
    pub max_idle_minutes: Option<u16>,
}

#[derive(Default)]
pub struct Planner {
    /// When each live window was first seen, for the new-window grace period.
    seen: HashMap<u64, u64>,
    /// When each live window was last used.
    active: HashMap<u64, u64>,
    pending: HashMap<String, Target>,
}

impl Planner {
    /// A window Winlane records as recently used, from the shared recent-window
    /// history. Focus changes that happen between scans still reset the clock.
    pub fn mark_active(&mut self, id: u64, now_ms: u64) {
        self.active.insert(id, now_ms);
    }

    /// How long a window has gone unused, counting from when it was first seen
    /// for windows Winlane has never watched being used.
    pub fn idle_ms(&self, id: u64, now_ms: u64) -> u64 {
        let since = self
            .active
            .get(&id)
            .or_else(|| self.seen.get(&id))
            .copied()
            .unwrap_or(now_ms);
        now_ms.saturating_sub(since)
    }

    pub fn observe(&mut self, snapshots: &[Snapshot], now_ms: u64) {
        let live: HashSet<_> = snapshots
            .iter()
            .flat_map(|s| s.windows.iter().map(|w| w.id))
            .collect();
        self.seen.retain(|id, _| live.contains(id));
        self.active.retain(|id, _| live.contains(id));
        for snapshot in snapshots {
            for window in &snapshot.windows {
                self.seen.entry(window.id).or_insert(now_ms);
                // A window that was already open when Winlane started counts
                // from the first scan that saw it, never from the epoch.
                self.active.entry(window.id).or_insert(now_ms);
                if snapshot.focused == Some(window.id) {
                    self.active.insert(window.id, now_ms);
                }
            }
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
            if !rule.limits_anything() || self.pending.contains_key(&rule.application.bundle_id) {
                continue;
            }
            let Some(snapshot) = snapshots
                .iter()
                .find(|s| s.bundle_id == rule.application.bundle_id)
            else {
                continue;
            };
            let eligible = |window: &&Window| {
                !window.protected
                    && window.server_id.is_some()
                    && self.seen.get(&window.id).is_some_and(|first| {
                        now_ms.saturating_sub(*first) >= settings.grace_period_ms()
                    })
            };
            let target = |reason, window: &Window| Target {
                bundle_id: snapshot.bundle_id.clone(),
                window: window.clone(),
                reason,
                max_windows: rule.max_windows,
                max_idle_minutes: rule.max_idle_minutes,
            };
            if rule
                .max_windows
                .is_some_and(|max| snapshot.windows.len() > usize::from(max))
                && let Some(window) = snapshot.windows.iter().filter(eligible).max_by_key(|w| {
                    (
                        ranks.get(&w.id).copied().unwrap_or(usize::MAX),
                        std::cmp::Reverse(w.id),
                    )
                })
            {
                return Some(target(Reason::WindowLimit, window));
            }
            if let Some(minutes) = rule.max_idle_minutes {
                let limit_ms = u64::from(minutes) * 60_000;
                // Close the window that has gone unused the longest, so a rule
                // never accidentally closes the one the user just left.
                if let Some(window) = snapshot
                    .windows
                    .iter()
                    .filter(eligible)
                    .filter(|w| self.idle_ms(w.id, now_ms) >= limit_ms)
                    .max_by_key(|w| (self.idle_ms(w.id, now_ms), std::cmp::Reverse(w.id)))
                {
                    return Some(target(Reason::Idle, window));
                }
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
