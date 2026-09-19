use super::*;

pub(super) fn verify_autosave(mtm: MainThreadMarker) {
    let previous_locale = winlane::core::i18n::locale();
    winlane::core::i18n::set_locale(winlane::core::i18n::Locale::English);
    let domain = NSString::from_str(&format!(
        "com.example.winlane-autosave-test-{}",
        std::process::id()
    ));
    let store = NSUserDefaults::initWithSuiteName(NSUserDefaults::alloc(), Some(&domain)).unwrap();
    store.removePersistentDomainForName(&domain);
    let delegate = Delegate::new(mtm);
    delegate.ivars().config_store.set(store.clone()).unwrap();
    let settings = delegate.ensure_settings_window();
    settings.fill(&Config::default());
    let saved = || {
        let text = store
            .stringForKey(ns_string!("WindowlanePreferencesV1"))
            .unwrap();
        let saved = Config::from_json(&text.to_string()).unwrap();
        assert_eq!(saved, *delegate.ivars().config.borrow());
        assert!(
            delegate.ivars().shortcut_tap.borrow().is_none(),
            "ordinary settings must not register a keyboard tap"
        );
        saved
    };
    crate::macos::ui::settings::tests::verify_autosave_controls(&settings, saved);
    crate::macos::ui::settings::tests::verify_sidebar_actions(&settings, saved);
    unsafe {
        let _: () = msg_send![&*delegate, resetSettings: None::<&AnyObject>];
    }
    assert_eq!(
        saved(),
        Config::default(),
        "Restore Defaults must persist immediately"
    );
    let seed = Config {
        app_shortcuts: vec![winlane::core::config::AppShortcut {
            application: ApplicationTarget {
                bundle_id: "com.example.browser".into(),
                path: "/Applications/Example Browser.app".into(),
                name: "Browser".into(),
            },
            shortcut: winlane::core::config::Shortcut {
                command: true,
                control: false,
                option: false,
                shift: false,
                key: "Digit1".into(),
            },
        }],
        ..Config::default()
    };
    delegate.ivars().config.replace(seed.clone());
    crate::macos::platform::preferences::save(&seed, &store).unwrap();
    let shortcuts = delegate.ensure_app_shortcuts_window();
    shortcuts.fill(&seed.app_shortcuts, &delegate, mtm);
    crate::macos::ui::app_shortcuts::tests::verify_autosave_target(
        &shortcuts, &delegate, mtm, saved,
    );
    assert!(!settings.window.isVisible() && !shortcuts.window.isVisible());
    let rules = delegate.ensure_alias_rules_window();
    crate::macos::ui::settings::tests::verify_escape_close(&rules.window);
    crate::macos::ui::alias_rules::tests::verify_rules_editor(&rules, &delegate, mtm, saved);
    assert_eq!(
        settings.candidate().unwrap().alias_rules,
        saved().alias_rules
    );
    crate::macos::ui::settings::tests::verify_escape_close(&settings.window);
    crate::macos::ui::settings::tests::verify_escape_close(&shortcuts.window);
    crate::macos::ui::settings::tests::verify_escape_autosave(&settings, saved);
    println!(
        "Settings Escape checks passed: all three windows close, active edits save, IME composition and modified Escape do not close windows."
    );
    store.removePersistentDomainForName(&domain);
    winlane::core::i18n::set_locale(previous_locale);
    println!(
        "Autosave checks passed: native actions persisted to an isolated preferences domain; invalid values and conflicts preserved prior settings; no keyboard taps installed."
    );
}
