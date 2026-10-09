//! Who is keeping this Mac awake, read from the power management assertions.
//!
//! macOS records one assertion per process and reason, and `pmset -g
//! assertions` prints them. Parsing that text is pure, so the interesting part
//! is testable without touching the machine's power state.

/// One power management assertion held by a process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blocker {
    pub pid: i32,
    /// Process name as the power manager prints it, for example `WeChatAppEx`.
    pub name: String,
    /// Assertion type, for example `NoDisplaySleepAssertion`.
    pub kind: String,
    /// `named: "…"` description, when the process supplied one.
    pub reason: String,
    /// How long the assertion has been held, in seconds.
    pub held_secs: u64,
}

impl Blocker {
    /// Whether this assertion keeps the machine from sleeping on its own.
    ///
    /// Display assertions count: while the display is on, the power manager
    /// holds its own "prevent sleep while display is on" assertion, so a
    /// process that keeps the screen awake also keeps the system awake. Input
    /// activity, background tasks, and network clients are short-lived and do
    /// not prevent idle sleep, so they are left out.
    pub fn prevents_sleep(&self) -> bool {
        const KINDS: [&str; 7] = [
            "PreventUserIdleSystemSleep",
            "NoIdleSleepAssertion",
            "PreventSystemSleep",
            "PreventUserIdleDisplaySleep",
            "NoDisplaySleepAssertion",
            "InternalPreventDisplaySleep",
            "ExternalMedia",
        ];
        KINDS.contains(&self.kind.as_str())
    }

    /// A short description for a status line, for example
    /// `WeChatAppEx (Video Wake Lock)`.
    pub fn label(&self) -> String {
        if self.reason.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, self.reason)
        }
    }
}

/// The idle rules that would close these apps' windows after `minutes` of
/// idle time, skipping apps that already have an Auto AppClose rule.
pub fn idle_rules_for(
    apps: &[crate::core::config::ApplicationTarget],
    existing: &[crate::features::auto_appclose::Rule],
    minutes: u16,
) -> Vec<crate::features::auto_appclose::Rule> {
    apps.iter()
        .filter(|app| {
            !existing
                .iter()
                .any(|rule| rule.application.bundle_id == app.bundle_id)
        })
        .map(|app| crate::features::auto_appclose::Rule {
            application: app.clone(),
            max_windows: None,
            max_idle_minutes: Some(minutes),
        })
        .collect()
}

/// Every assertion in the output, in the order the power manager printed them.
pub fn parse(assertions: &str) -> Vec<Blocker> {
    let mut blockers = Vec::new();
    for line in assertions.lines() {
        let Some(rest) = line.trim_start().strip_prefix("pid ") else {
            continue;
        };
        let Some((pid, rest)) = rest.split_once('(') else {
            continue;
        };
        let Ok(pid) = pid.trim().parse() else {
            continue;
        };
        let Some((name, rest)) = rest.split_once(')') else {
            continue;
        };
        // `: [0x…] 00:12:34 Kind named: "reason"`. Rows without a held time or
        // a kind are summaries, not assertions.
        let Some(rest) = rest.trim_start().strip_prefix(':') else {
            continue;
        };
        let tokens: Vec<&str> = rest.split_whitespace().collect();
        // The bracketed identifier is only printed inside a live session.
        let held_index = usize::from(tokens.first().is_some_and(|token| token.starts_with('[')));
        let Some(held) = tokens
            .get(held_index)
            .and_then(|token| parse_duration(token))
        else {
            continue;
        };
        let Some(kind) = tokens.get(held_index + 1) else {
            continue;
        };
        let reason = rest
            .split_once("named: ")
            .and_then(|(_, reason)| reason.trim().strip_prefix('"'))
            .and_then(|reason| reason.split_once('"').map(|(reason, _)| reason))
            .unwrap_or_default();
        blockers.push(Blocker {
            pid,
            name: name.trim().to_owned(),
            kind: (*kind).to_owned(),
            reason: reason.to_owned(),
            held_secs: held,
        });
    }
    blockers
}

/// `00:12:34` as seconds.
fn parse_duration(token: &str) -> Option<u64> {
    let mut parts = token.split(':');
    let hours: u64 = parts.next()?.parse().ok()?;
    let minutes: u64 = parts.next()?.parse().ok()?;
    let seconds: u64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(hours * 3600 + minutes * 60 + seconds)
}
