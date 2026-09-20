use super::*;
use objc2_foundation::{NSDistributedNotificationCenter, NSNotificationSuspensionBehavior};

impl Delegate {
    pub(super) fn observe_input_indicator(&self) {
        // SAFETY: This application-lifetime delegate handles distributed Carbon
        // notifications on the main thread, including when Winlane is inactive.
        unsafe {
            for (name, selector) in [
                (
                    input_source::selection_notification(),
                    sel!(appInputSourceChanged:),
                ),
                (
                    input_source::selection_notification(),
                    sel!(indicatorSourceChanged:),
                ),
                (
                    input_source::sources_notification(),
                    sel!(indicatorSourcesChanged:),
                ),
            ] {
                NSDistributedNotificationCenter::defaultCenter()
                    .addObserver_selector_name_object_suspensionBehavior(
                        self,
                        selector,
                        Some(&name),
                        None,
                        NSNotificationSuspensionBehavior::DeliverImmediately,
                    );
            }
            NSWorkspace::sharedWorkspace()
                .notificationCenter()
                .addObserver_selector_name_object(
                    self,
                    sel!(indicatorSourceChanged:),
                    Some(NSWorkspaceActiveSpaceDidChangeNotification),
                    None,
                );
            NSWorkspace::sharedWorkspace()
                .notificationCenter()
                .addObserver_selector_name_object(
                    self,
                    sel!(indicatorSourceChanged:),
                    Some(NSWorkspaceDidWakeNotification),
                    None,
                );
        }
    }

    pub(super) fn update_input_indicator(&self) {
        let config = self.ivars().config.borrow();
        if !config.input_indicator.enabled {
            self.ivars().input_indicator.take();
            return;
        }
        let source = Source::current(self.mtm()).and_then(|source| source.description());
        let screens = crate::macos::ui::input_indicator::screens(self.mtm());
        let mut state = self.ivars().input_indicator.borrow_mut();
        let indicator = state.get_or_insert_with(Default::default);
        indicator.configure(
            &config.input_indicator,
            source.as_ref(),
            &screens,
            self.mtm(),
        );
        indicator.show();
    }
}
