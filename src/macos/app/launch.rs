use super::*;

impl Delegate {
    pub(super) fn launch_app_shortcut(&self, index: usize) {
        let Some(item) = self
            .ivars()
            .config
            .borrow()
            .app_shortcuts
            .get(index)
            .cloned()
        else {
            return;
        };
        self.launch_application(&item.application, LaunchOrigin::Shortcut);
    }

    pub(super) fn launch_application(&self, application: &ApplicationTarget, origin: LaunchOrigin) {
        self.remember_frontmost_window();
        self.end_session();
        self.ivars().launch_receiver.replace(None);
        if let Some(window) = self.ivars().alias_rules.borrow().clone() {
            window.window.orderOut(None);
        }
        if let Some(settings) = self.settings_window() {
            settings.window.orderOut(None);
        }
        if let Some(settings) = self.app_shortcuts_window() {
            settings.window.orderOut(None);
        }
        match crate::macos::platform::applications::launch(
            application,
            self.ivars().wake.get().unwrap().handle(),
        ) {
            Ok(receiver) => {
                self.ivars()
                    .launch_receiver
                    .replace(Some(PendingLaunch { receiver, origin }));
            }
            Err(error) => self.report_launch_error(&error, origin),
        }
    }

    pub(super) fn poll_app_launch(&self) {
        let result = self
            .ivars()
            .launch_receiver
            .borrow()
            .as_ref()
            .map(|pending| (pending.receiver.try_recv(), pending.origin));
        let (result, origin) = match result {
            Some((Ok(result), origin)) => (result, origin),
            Some((Err(TryRecvError::Disconnected), origin)) => (
                Err(tr!(
                    "应用启动未完成，请重试。",
                    "The app did not finish launching. Try again."
                )
                .into()),
                origin,
            ),
            _ => return,
        };
        self.ivars().launch_receiver.replace(None);
        match result {
            Ok(pid) => {
                if let Some(app) =
                    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
                {
                    app.unhide();
                    self.remember_application(pid);
                } else {
                    self.report_launch_error(
                        tr!(
                            "应用启动后已退出，请检查应用状态。",
                            "The app exited after launching. Check its status."
                        ),
                        origin,
                    )
                }
            }
            Err(error) => self.report_launch_error(&error, origin),
        }
    }

    pub(super) fn report_launch_error(&self, error: &str, origin: LaunchOrigin) {
        match origin {
            LaunchOrigin::Shortcut => {
                self.show_app_shortcuts();
                if let Some(window) = self.app_shortcuts_window() {
                    window.report(error, true);
                }
            }
            LaunchOrigin::Search => {
                self.display_search(self.ivars().session.get());
                self.present_panels();
                self.report_switch_error(error);
            }
        }
    }
}
