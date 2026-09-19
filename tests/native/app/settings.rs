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
    assert!(delegate.ivars().settings_release_pending.get());
    delegate.release_closed_settings();
    assert!(delegate.settings_window().is_none());
    println!(
        "Settings Escape checks passed: all three windows close, active edits save, IME composition and modified Escape do not close windows."
    );
    store.removePersistentDomainForName(&domain);
    winlane::core::i18n::set_locale(previous_locale);
    println!(
        "Autosave checks passed: native actions persisted to an isolated preferences domain; invalid values and conflicts preserved prior settings; no keyboard taps installed."
    );
    verify_settings_lifecycle(mtm);
}

fn verify_settings_lifecycle(mtm: MainThreadMarker) {
    use crate::macos::ui::settings::tests::verify_loaded_pages;
    use objc2::rc::autoreleasepool;

    let delegate = Delegate::new(mtm);
    let initial = Config {
        snippets: vec![winlane::features::snippets::Snippet {
            id: "greeting".into(),
            name: "Greeting".into(),
            body: "Hello {clipboard}".into(),
        }],
        quicklinks: vec![winlane::features::quicklinks::Quicklink {
            id: "docs".into(),
            name: "Docs".into(),
            link: "https://example.com/docs".into(),
            open_with: String::new(),
            shortcut: None,
        }],
        ..Config::default()
    };
    delegate.ivars().config.replace(initial.clone());
    let (window, snippet_view, quicklink_view, snippet, quicklink, snippet_draft, quicklink_draft) =
        autoreleasepool(|_| {
            let settings = delegate.ensure_settings_window();
            verify_loaded_pages(&settings, &[4]);
            assert!(delegate.ivars().snippet_editor.borrow().is_none());
            assert!(delegate.ivars().quicklink_editor.borrow().is_none());
            assert_eq!(settings.candidate().unwrap(), initial);
            settings.select_tab(6);
            let snippet = delegate.ivars().snippet_editor.borrow().clone().unwrap();
            assert!(delegate.ivars().quicklink_editor.borrow().is_none());
            settings.select_tab(8);
            let quicklink = delegate.ivars().quicklink_editor.borrow().clone().unwrap();
            // Unfinished additions are drafts; they must survive without being saved.
            unsafe {
                let _: () = msg_send![&*snippet, addSnippet: None::<&AnyObject>];
                let _: () = msg_send![&*quicklink, addQuicklink: None::<&AnyObject>];
            }
            assert_eq!(*delegate.ivars().config.borrow(), initial);
            verify_loaded_pages(&settings, &[4, 6, 8]);
            let result = (
                Weak::new(&*settings.window),
                Weak::new(snippet.view()),
                Weak::new(quicklink.view()),
                Weak::new(&*snippet),
                Weak::new(&*quicklink),
                snippet.draft(),
                quicklink.draft(),
            );
            settings.window.close();
            assert!(delegate.ivars().settings_release_pending.get());
            assert!(
                delegate.settings_window().is_some(),
                "release must wait until close returns"
            );
            delegate.release_closed_settings();
            assert!(delegate.settings_window().is_none());
            assert!(delegate.ivars().snippet_editor.borrow().is_none());
            assert!(delegate.ivars().quicklink_editor.borrow().is_none());
            result
        });
    assert!(
        window.load().is_none(),
        "closed Settings window was retained"
    );
    assert!(
        snippet_view.load().is_none(),
        "snippet controls were retained"
    );
    assert!(
        quicklink_view.load().is_none(),
        "quicklink controls were retained"
    );
    assert!(snippet.load().is_none());
    assert!(quicklink.load().is_none());

    for _ in 0..2 {
        let (window, snippet, quicklink) = autoreleasepool(|_| {
            let settings = delegate.ensure_settings_window();
            assert_eq!(settings.selected_tab(), 8);
            verify_loaded_pages(&settings, &[8]);
            assert!(delegate.ivars().snippet_editor.borrow().is_none());
            assert!(delegate.ivars().snippet_editor_draft.borrow().is_some());
            let quicklink = delegate.ivars().quicklink_editor.borrow().clone().unwrap();
            assert_eq!(quicklink.draft(), quicklink_draft);
            settings.select_tab(6);
            let snippet = delegate.ivars().snippet_editor.borrow().clone().unwrap();
            assert_eq!(snippet.draft(), snippet_draft);
            assert_eq!(settings.candidate().unwrap(), initial);
            assert_eq!(*delegate.ivars().config.borrow(), initial);
            verify_loaded_pages(&settings, &[6, 8]);
            settings.select_tab(8);
            let weak = (
                Weak::new(&*settings.window),
                Weak::new(&*snippet),
                Weak::new(&*quicklink),
            );
            settings.window.close();
            delegate.release_closed_settings();
            weak
        });
        assert!(window.load().is_none());
        assert!(snippet.load().is_none());
        assert!(quicklink.load().is_none());
    }
    println!(
        "Settings lifecycle checks passed: pages load on demand; windows and editors release on close; drafts and saved values survive repeated reopen."
    );
}
