use super::*;

impl Delegate {
    pub(super) fn configure_app_input_rules(&self) {
        self.ivars().app_input_rules.borrow_mut().reset_activation();
        self.request_app_input_rules();
    }

    pub(super) fn request_app_input_rules(&self) {
        if NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .is_some_and(|app| app.processIdentifier() == std::process::id() as i32)
        {
            self.ivars().app_input_rules.borrow_mut().reset_activation();
        }
        self.ivars().app_input_pending.set(true);
        self.update_app_input_rules();
    }

    fn app_input_blocked(&self) -> bool {
        self.ivars().demo.get()
            || self.ivars().preparing_panel.get()
            || self.ivars().mode.get().is_some()
            || NSApplication::sharedApplication(self.mtm())
                .keyWindow()
                .is_some()
    }

    pub(super) fn update_app_input_rules(&self) {
        if !self.ivars().app_input_pending.get() || self.app_input_blocked() {
            return;
        }
        self.ivars().app_input_pending.set(false);
        let app = NSWorkspace::sharedWorkspace().frontmostApplication();
        let app =
            app.filter(|app| app.activationPolicy() == NSApplicationActivationPolicy::Regular);
        let id = app
            .as_ref()
            .and_then(|app| app.bundleIdentifier())
            .map(|id| id.to_string());
        let identity = id
            .as_deref()
            .zip(app.as_ref().map(|app| app.processIdentifier()));
        let current = Source::current(self.mtm()).and_then(|source| source.id());
        let target = self.ivars().app_input_rules.borrow_mut().activate(
            &self.ivars().config.borrow().input_rules,
            identity,
            current.as_deref(),
            |rule| input_source::resolve_rule(rule, self.mtm()).and_then(|source| source.id()),
        );
        if let Some(source) = target.and_then(|id| Source::by_id(&id, self.mtm())) {
            source.select(self.mtm());
        }
        self.remember_app_input();
    }

    pub(super) fn remember_app_input(&self) {
        if self.app_input_blocked() {
            return;
        }
        let Some(app) = NSWorkspace::sharedWorkspace().frontmostApplication() else {
            return;
        };
        let Some(id) = app.bundleIdentifier() else {
            return;
        };
        let Some(source) = Source::current(self.mtm()).and_then(|source| source.id()) else {
            return;
        };
        self.ivars().app_input_rules.borrow_mut().observe(
            &self.ivars().config.borrow().input_rules,
            &id.to_string(),
            app.processIdentifier(),
            &source,
        );
    }

    pub(super) fn restore_app_input_history(&self) {
        if let Some(text) = NSUserDefaults::standardUserDefaults()
            .stringForKey(ns_string!("WinlaneAppInputHistoryV1"))
        {
            self.ivars()
                .app_input_rules
                .borrow_mut()
                .restore_history(&text.to_string());
        }
    }

    pub(super) fn save_app_input_history(&self) {
        let text = NSString::from_str(&self.ivars().app_input_rules.borrow().history());
        // SAFETY: Only app identifiers and input source identifiers are persisted.
        unsafe {
            NSUserDefaults::standardUserDefaults()
                .setObject_forKey(Some(&text), ns_string!("WinlaneAppInputHistoryV1"));
        }
    }
}
