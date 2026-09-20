use super::*;

#[derive(Clone, Copy)]
pub(super) struct PendingSettingsFocus {
    pub(super) started: Instant,
    pub(super) origin_pid: Option<i32>,
}

impl Delegate {
    pub(super) fn cancel_settings_focus(&self) {
        self.ivars().settings_focus_pending.set(None);
        if let Some(timer) = self.ivars().settings_focus_timer.take() {
            timer.invalidate();
        }
    }

    pub(super) fn request_settings_focus(&self, settings: &SettingsWindow) {
        self.cancel_settings_focus();
        self.ivars()
            .settings_focus_pending
            .set(Some(PendingSettingsFocus {
                started: Instant::now(),
                origin_pid: NSWorkspace::sharedWorkspace()
                    .frontmostApplication()
                    .map(|app| app.processIdentifier()),
            }));
        // Keep the request alive past the event that releases the nonactivating panel.
        // App activation and the Settings window receiving keyboard focus can finish separately.
        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                0.1,
                self,
                sel!(retrySettingsFocus:),
                None,
                true,
            )
        };
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
        self.ivars().settings_focus_timer.replace(Some(timer));
        let app = NSApplication::sharedApplication(self.mtm());
        if app.isHidden() {
            app.unhideWithoutActivation();
        }
        settings.bring_to_front();
        app.activate();
    }

    pub(super) fn finish_settings_focus(&self, after_dispatch: bool) {
        if self.ivars().settings_focus_pending.get().is_none() {
            return;
        }
        self.check_settings_focus(
            NSApplication::sharedApplication(self.mtm()).isActive(),
            NSWorkspace::sharedWorkspace()
                .frontmostApplication()
                .map(|app| app.processIdentifier()),
            Instant::now(),
            after_dispatch,
        );
    }

    pub(super) fn check_settings_focus(
        &self,
        active: bool,
        frontmost_pid: Option<i32>,
        now: Instant,
        after_dispatch: bool,
    ) {
        let Some(request) = self.ivars().settings_focus_pending.get() else {
            return;
        };
        let Some(settings) = self.settings_window() else {
            self.cancel_settings_focus();
            return;
        };
        let another_app = frontmost_pid
            .is_some_and(|pid| pid != std::process::id() as i32 && Some(pid) != request.origin_pid);
        if self.ivars().mode.get().is_some()
            || self.ivars().settings_release_pending.get()
            || now.saturating_duration_since(request.started) >= Duration::from_millis(1500)
            || another_app
        {
            self.cancel_settings_focus();
            return;
        }
        let application_front = active && frontmost_pid == Some(std::process::id() as i32);
        let focused = application_front
            && settings.window.isVisible()
            && settings.window.isOnActiveSpace()
            && (settings.window.isKeyWindow()
                || settings
                    .window
                    .attachedSheet()
                    .is_some_and(|sheet| sheet.isKeyWindow()));
        if focused {
            // A synchronous key/activation callback can precede AppKit's panel teardown.
            // Only a later run-loop check confirms that the handoff has settled.
            if after_dispatch {
                self.cancel_settings_focus();
            }
            return;
        }
        settings.bring_to_front();
        if after_dispatch && !application_front {
            NSApplication::sharedApplication(self.mtm()).activate();
        }
    }
}
