use super::*;

impl Delegate {
    pub(super) fn update_time_indicator(&self) {
        self.configure_time_indicator();
    }

    fn configure_time_indicator(&self) {
        let config = self.ivars().config.borrow();
        if !config.time_indicator.enabled {
            self.ivars().time_indicator.take();
            return;
        }
        let screens = crate::macos::ui::input_indicator::screens(self.mtm());
        let mut state = self.ivars().time_indicator.borrow_mut();
        let indicator = state.get_or_insert_with(Default::default);
        indicator.configure(&config.time_indicator, &screens, self.mtm());
        indicator.show();
    }
}
