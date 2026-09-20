use super::*;
use crate::macos::platform::scrolling::ScrollTap;

impl Delegate {
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
                "滚动监听已停止。检查系统授权后点击重试。",
                "Scroll monitoring stopped. Check system permissions, then retry."
            ))
        };
        settings.update_scrolling_status(text, enabled && !active);
    }
}
