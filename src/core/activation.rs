//! Bounded window-activation decisions. A successful API return is not evidence
//! of foreground focus; only fresh observations can complete a request.
use std::time::Duration;

pub const VERIFY_INTERVAL: Duration = Duration::from_millis(50);
pub const ACTIVATION_TIMEOUT: Duration = Duration::from_millis(1500);
const FALLBACK_AFTER: Duration = Duration::from_millis(250);
const RAISE_INTERVAL: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowState {
    pub focused_id: Option<u64>,
    pub focused_server_id: Option<u32>,
    pub on_screen: Option<bool>,
    pub alive: Option<bool>,
    pub ax_error: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Observation {
    pub front_pid: Option<i32>,
    pub active: bool,
    pub running: bool,
    pub window: WindowState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    TimedOut,
    AppExited,
    WindowClosed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Wait,
    Raise,
    Reopen,
    Confirmed,
    Cancelled,
    Failed(Failure),
}

pub struct Activation {
    pub target_pid: i32,
    pub window_id: u64,
    pub server_id: Option<u32>,
    pub origin_pid: Option<i32>,
    pub self_pid: i32,
    pub request_allowed: bool,
    pub fallback: bool,
    pub fallback_complete: bool,
    pub raises: u8,
    seen_target: bool,
    last_raise: Option<Duration>,
    matching_since: Option<Duration>,
}

impl Activation {
    pub fn new(
        target_pid: i32,
        window_id: u64,
        server_id: Option<u32>,
        origin_pid: Option<i32>,
        self_pid: i32,
    ) -> Self {
        Self {
            target_pid,
            window_id,
            server_id,
            origin_pid,
            self_pid,
            request_allowed: true,
            fallback: false,
            fallback_complete: false,
            raises: 0,
            seen_target: false,
            last_raise: None,
            matching_since: None,
        }
    }

    /// Once the target has come forward, returning to the original app is also
    /// a new user intention, not permission to steal focus back.
    pub fn interrupted(&mut self, front: Option<i32>) -> bool {
        if front == Some(self.target_pid) {
            self.seen_target = true;
            return false;
        }
        front.is_some_and(|pid| {
            pid != self.self_pid && (Some(pid) != self.origin_pid || self.seen_target)
        })
    }

    pub fn window_matches(&self, state: WindowState) -> bool {
        state.focused_id == Some(self.window_id)
            || self
                .server_id
                .is_some_and(|id| state.focused_server_id == Some(id))
    }

    pub fn stable_ms(&self, elapsed: Duration) -> u128 {
        self.matching_since
            .map_or(0, |since| elapsed.saturating_sub(since).as_millis())
    }

    /// `busy` forbids new actions and confirmation while a previous write or
    /// LaunchServices completion is outstanding. Timeouts/cancellation still run.
    pub fn decide(&mut self, elapsed: Duration, observation: Observation, busy: bool) -> Decision {
        if self.interrupted(observation.front_pid) {
            return Decision::Cancelled;
        }
        if !observation.running {
            return Decision::Failed(Failure::AppExited);
        }
        if observation.window.alive == Some(false) {
            return Decision::Failed(Failure::WindowClosed);
        }
        if elapsed >= ACTIVATION_TIMEOUT {
            return Decision::Failed(Failure::TimedOut);
        }
        if busy || (self.fallback && !self.fallback_complete) {
            return Decision::Wait;
        }
        let front = observation.front_pid == Some(self.target_pid) && observation.active;
        if front
            && self.window_matches(observation.window)
            && observation.window.on_screen != Some(false)
        {
            if self
                .matching_since
                .is_some_and(|since| elapsed.saturating_sub(since) >= VERIFY_INTERVAL)
            {
                return Decision::Confirmed;
            }
            self.matching_since.get_or_insert(elapsed);
            return Decision::Wait;
        }
        self.matching_since = None;
        if !self.fallback
            && (!self.request_allowed
                || (elapsed >= FALLBACK_AFTER && (!front || self.raises >= 2)))
        {
            self.fallback = true;
            self.raises = 0;
            self.last_raise = None;
            return Decision::Reopen;
        }
        if front
            && self.raises < 2
            && self
                .last_raise
                .is_none_or(|last| elapsed.saturating_sub(last) >= RAISE_INTERVAL)
        {
            self.raises += 1;
            self.last_raise = Some(elapsed);
            return Decision::Raise;
        }
        Decision::Wait
    }
}
