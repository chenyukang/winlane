use super::*;
use crate::macos::platform::scrolling::ScrollTap;
use std::time::{Duration, Instant};

/// A tap that cannot start yet is retried on this interval rather than on every
/// poll pass.
pub(super) const SCROLL_RETRY_INTERVAL: Duration = Duration::from_secs(3);

/// Whether the poll may try to install the scroll tap again.
pub(super) fn retry_due(last_attempt: Option<Instant>, now: Instant) -> bool {
    last_attempt.is_none_or(|at| now.duration_since(at) >= SCROLL_RETRY_INTERVAL)
}

/// What the poll has to do about the scroll tap. Kept apart from the work so
/// every case can be checked without a real permission change.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(super) enum ScrollingHealth {
    /// The setting is off.
    Disabled,
    /// Winlane is not allowed to watch input. A tap installed before access was
    /// taken away still reports as enabled while it cannot see anything, so it
    /// is dropped and installed again once access comes back.
    Unpermitted,
    /// The setting is on and no tap is running.
    Missing,
    /// A tap is installed and running.
    Active,
}

pub(super) fn scrolling_health(trusted: bool, enabled: bool, tap_active: bool) -> ScrollingHealth {
    if !enabled {
        ScrollingHealth::Disabled
    } else if !trusted {
        ScrollingHealth::Unpermitted
    } else if tap_active {
        ScrollingHealth::Active
    } else {
        ScrollingHealth::Missing
    }
}

impl Delegate {
    /// Install the scroll tap again when it is missing. Permission is checked
    /// while the app starts, so without this the setting stayed dead after the
    /// user granted Accessibility access back until they pressed Retry.
    pub(super) fn retry_scrolling_if_needed(&self) {
        let health = scrolling_health(
            accessibility::is_trusted(),
            self.ivars().config.borrow().scrolling.enabled,
            self.ivars()
                .scroll_tap
                .borrow()
                .as_ref()
                .is_some_and(ScrollTap::is_enabled),
        );
        match health {
            ScrollingHealth::Unpermitted => {
                if self.ivars().scroll_tap.borrow_mut().take().is_some() {
                    self.ivars().scroll_error.replace(Some(
                        tr!(
                            "滚动监听已停止，恢复系统授权后会自动重试。",
                            "Scroll monitoring stopped until Winlane is allowed again, then it retries by itself."
                        )
                        .into(),
                    ));
                    self.update_scrolling_status();
                }
            }
            ScrollingHealth::Missing => {
                let now = Instant::now();
                if !retry_due(self.ivars().scroll_retry_at.get(), now) {
                    return;
                }
                self.ivars().scroll_retry_at.set(Some(now));
                self.ensure_scrolling();
            }
            ScrollingHealth::Disabled | ScrollingHealth::Active => {}
        }
    }

    pub(super) fn ensure_scrolling(&self) {
        let settings = self.ivars().config.borrow().scrolling.clone();
        if !settings.enabled {
            self.ivars().scroll_tap.take();
            self.ivars().scroll_error.take();
        } else if self
            .ivars()
            .scroll_tap
            .borrow()
            .as_ref()
            .is_none_or(|tap| !tap.is_enabled())
        {
            match ScrollTap::prepare(&settings, self.mtm()).and_then(|tap| {
                tap.start()?;
                Ok(tap)
            }) {
                Ok(tap) => {
                    self.ivars().scroll_tap.replace(Some(tap));
                    self.ivars().scroll_error.take();
                }
                Err(error) => {
                    self.ivars().scroll_error.replace(Some(error));
                }
            }
        }
        self.update_scrolling_status();
    }

    pub(super) fn update_scrolling_status(&self) {
        let Some(settings) = self.settings_window() else {
            return;
        };
        let enabled = self.ivars().config.borrow().scrolling.enabled;
        let active = self
            .ivars()
            .scroll_tap
            .borrow()
            .as_ref()
            .is_some_and(ScrollTap::is_enabled);
        let error = self.ivars().scroll_error.borrow();
        let text = if !enabled {
            tr!(
                "未启用，使用系统滚动行为。",
                "Disabled. Using system scrolling."
            )
        } else if active {
            tr!("已启用。", "Active.")
        } else {
            error.as_deref().unwrap_or(tr!(
                "滚动监听已停止。恢复系统授权后会自动重试，也可以点“重试”。",
                "Scroll monitoring stopped. It retries by itself once system access is back, or press Retry."
            ))
        };
        settings.update_scrolling_status(text, enabled && !active);
    }
}
