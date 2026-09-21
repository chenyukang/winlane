use super::*;
use crate::macos::platform::preferences::save;
use objc2_foundation::{NSUserDefaults, ns_string};
use winlane::core::i18n;

pub fn verify_localized_settings(target: &AnyObject, mtm: MainThreadMarker) {
    verify_lazy_pages(target, mtm);
    verify_card_appearances(mtm);
    input::tests::verify(target, mtm);
    input_rules::tests::verify(target, mtm);
    scrolling::tests::verify(target, mtm);
    files::tests::verify(target, mtm);
    auto_cleanup::tests::verify(target, mtm);
    use winlane::core::i18n::Locale;
    for (locale, title, tabs) in [
        (
            Locale::English,
            "Winlane Settings",
            [
                "Shortcuts",
                "Appearance",
                "Input Indicator",
                "Windows",
                "General",
                "Aliases",
                "Snippets",
                "Clipboard",
                "Quicklinks",
                "Input Rules",
                "Mouse Scrolling",
                "Files",
                "Auto Cleanup",
            ],
        ),
        (
            Locale::Chinese,
            "Winlane 设置",
            [
                "快捷键",
                "外观",
                "输入法指示器",
                "窗口列表",
                "常规",
                "Alias 规则",
                "文本片段",
                "剪贴板",
                "快捷链接",
                "输入法规则",
                "鼠标滚轮",
                "文件",
                "自动清理",
            ],
        ),
    ] {
        i18n::set_locale(locale);
        let settings = SettingsWindow::new(target, mtm);
        settings.update_updater(false, false, None);
        assert!(!settings.general().automatic_updates.isEnabled());
        assert!(!settings.general().check_updates.isEnabled());
        settings.update_updater(true, true, None);
        assert!(settings.general().automatic_updates.isEnabled());
        assert_eq!(
            settings.general().automatic_updates.state(),
            NSControlStateValueOn
        );
        assert_eq!(
            settings.general().automatic_updates.action(),
            Some(sel!(toggleAutomaticUpdates:))
        );
        assert_eq!(
            settings.general().check_updates.action(),
            Some(sel!(checkForUpdates:))
        );
        settings.update_updater(true, false, None);
        assert_eq!(
            settings.general().automatic_updates.state(),
            NSControlStateValueOff
        );
        settings.update_updater(false, false, Some("invalid feed"));
        assert!(settings.general().check_updates.isEnabled());
        for control in [
            &*settings.general().language,
            &*settings.appearance().appearance,
            &*settings.appearance().density,
            &*settings.windows().sort,
        ] {
            assert_eq!(control.action(), Some(sel!(settingsChanged:)));
        }
        for shortcut in [
            &settings.shortcuts().search_shortcut,
            &settings.shortcuts().switch_shortcut,
        ] {
            assert_eq!(shortcut.key.action(), Some(sel!(settingsChanged:)));
            assert!(
                shortcut
                    .modifiers
                    .iter()
                    .all(|control| control.action() == Some(sel!(settingsChanged:)))
            );
        }
        assert!(
            settings
                .windows()
                .excluded
                .cell()
                .unwrap()
                .sendsActionOnEndEditing()
        );
        assert_eq!(
            settings.appearance().usage_hints.action(),
            Some(sel!(settingsChanged:))
        );
        assert_eq!(
            settings.appearance().usage_hints.title().to_string(),
            match locale {
                Locale::English => "Show footer hints and Settings button",
                Locale::Chinese => "显示底部提示和设置按钮",
            }
        );
        assert!(
            settings
                .appearance()
                .opacity_input
                .cell()
                .unwrap()
                .sendsActionOnEndEditing()
        );
        assert!(
            settings
                .window
                .contentView()
                .unwrap()
                .subviews()
                .iter()
                .all(|view| {
                    view.downcast_ref::<NSButton>().is_none_or(|button| {
                        button.title().to_string() != "Save Settings"
                            && button.title().to_string() != "保存设置"
                    })
                })
        );
        assert!(!settings.candidate().unwrap().debug_logging);
        settings
            .general()
            .debug_logging
            .setState(NSControlStateValueOn);
        assert!(settings.candidate().unwrap().debug_logging);
        settings
            .general()
            .debug_logging
            .setState(NSControlStateValueOff);
        assert_eq!(
            settings.general().debug_logging.action(),
            Some(sel!(settingsChanged:))
        );
        assert_eq!(settings.window.title().to_string(), title);
        assert_eq!(settings.tabs.tabViewType(), NSTabViewType::NoTabsNoBorder);
        assert_eq!(settings.selected_tab(), 4, "open settings on General");
        let size = settings.window.contentView().unwrap().frame().size;
        assert_eq!(
            settings
                .navigation
                .iter()
                .map(|button| button.tag())
                .collect::<Vec<_>>(),
            [4, 1, 0, 9, 2, 3, 12, 5, 6, 7, 8, 10, 11]
        );
        for button in &settings.navigation {
            assert_eq!(button.action(), Some(sel!(selectSettingsSection:)));
            assert!(
                button.ivars().symbol.is_some(),
                "navigation symbol must exist"
            );
            assert!(
                button
                    .target()
                    .is_some_and(|value| std::ptr::eq(&*value, target))
            );
        }
        for (index, title) in tabs.iter().enumerate() {
            settings.select_tab(index as isize);
            let item = settings.tabs.selectedTabViewItem().unwrap();
            assert_eq!(item.label().to_string(), *title);
            assert_eq!(settings.selected_tab(), index as isize);
            assert_eq!(settings.page_title.stringValue().to_string(), *title);
            assert_eq!(settings.page_description.isHidden(), index == 0);
            assert_eq!(
                settings.page_description.stringValue().is_empty(),
                index == 0
            );
            assert_eq!(
                settings.window.contentView().unwrap().frame().size,
                size,
                "switching sections must not resize the window"
            );
            let selected: Vec<_> = settings
                .navigation
                .iter()
                .filter(|button| button.state() == NSControlStateValueOn)
                .map(|button| button.tag())
                .collect();
            assert_eq!(selected, [index as isize]);
            settings
                .window
                .contentView()
                .unwrap()
                .layoutSubtreeIfNeeded();
            let view = item.view(mtm).unwrap();
            verify_settings_bounds(&view);
            if index == 2 {
                let subviews = view.subviews();
                let scroll = subviews.objectAtIndex(0);
                let scroll = scroll.downcast_ref::<NSScrollView>().unwrap();
                verify_settings_bounds(&scroll.documentView().unwrap());
                assert!(
                    (scroll.documentVisibleRect().origin.y
                        + scroll.documentVisibleRect().size.height
                        - scroll.documentView().unwrap().bounds().size.height)
                        .abs()
                        < 1.0,
                    "Input opens at the top of its scrollable page"
                );
            }
            for child in view.subviews() {
                let frame = child.frame();
                assert!(frame.origin.x >= 0.0 && frame.origin.y >= 0.0);
                assert!(
                    (frame.origin.x + frame.size.width) <= view.bounds().size.width,
                    "control exceeds tab width: {frame:?}"
                );
                assert!(
                    (frame.origin.y + frame.size.height) <= view.bounds().size.height,
                    "control exceeds tab height: {frame:?}"
                );
            }
        }
        let mut config = Config::default();
        use winlane::core::commands::CommandId;
        use winlane::core::config::CommandShortcut;
        let command_binding = Shortcut {
            command: true,
            option: true,
            control: false,
            shift: false,
            key: "KeyU".into(),
        };
        config.command_shortcuts = vec![CommandShortcut {
            command: CommandId::OpenUrl,
            shortcut: command_binding.clone(),
        }];
        settings.fill(&config);
        assert_eq!(settings.candidate().unwrap(), config);
        let (_, controls) = settings
            .shortcuts()
            .command_shortcuts
            .rows
            .iter()
            .find(|(id, _)| *id == CommandId::OpenUrl)
            .unwrap();
        assert_eq!(controls.key.action(), Some(sel!(settingsChanged:)));
        assert!(
            controls
                .modifiers
                .iter()
                .all(|control| control.action() == Some(sel!(settingsChanged:)))
        );
        controls.fill_optional(Some(&config.shortcut));
        let error = settings
            .candidate()
            .expect_err("command shortcut conflicts must prevent saving");
        assert!(error.contains("open-url"));
        assert!(error.contains(tr!("搜索模式（快捷键 1）", "Search mode (shortcut 1)")));
        assert!(error.contains(&config.shortcut.display()));
        settings.report(&error, true);
        assert_eq!(settings.message.toolTip().unwrap().to_string(), error);
        settings.report("", false);
        assert!(settings.message.toolTip().is_none());
        controls.fill_optional(None);
        assert!(settings.candidate().unwrap().command_shortcuts.is_empty());
        config.command_shortcuts.clear();
        config.clipboard.enabled = false;
        config.clipboard.persistent = false;
        config.clipboard.max_items = 500;
        config.clipboard.retention_days = 30;
        settings.fill(&config);
        assert_eq!(settings.candidate().unwrap(), config);
        config.clipboard.max_items = 0;
        settings.fill(&config);
        assert!(settings.candidate().is_err());
        for (index, language) in [Language::System, Language::Chinese, Language::English]
            .into_iter()
            .enumerate()
        {
            let config = Config {
                language,
                excluded_apps: vec!["Example".into()],
                appearance: Appearance::Dark,
                show_usage_hints: index % 2 == 0,
                ..Config::default()
            };
            settings.fill(&config);
            assert_eq!(settings.candidate().unwrap(), config);
            assert_eq!(
                settings.general().language.indexOfSelectedItem(),
                index as isize
            );
        }
        settings.general().language.selectItemAtIndex(0);
        assert_eq!(settings.candidate().unwrap().language, Language::System);
        for input_method in [
            InputMethod::Current,
            InputMethod::English,
            InputMethod::Chinese,
            InputMethod::LastUsed,
        ] {
            let config = Config {
                input_rules: winlane::features::input_rules::Settings::for_winlane(input_method),
                ..Config::default()
            };
            settings.fill(&config);
            assert_eq!(
                settings.candidate().unwrap().input_rules.winlane_policy(),
                input_method
            );
            assert_eq!(settings.candidate().unwrap(), config);
        }
        settings.fill(&Config::default());
        assert_eq!(
            settings.candidate().unwrap().input_rules.winlane_policy(),
            InputMethod::English
        );
        assert_eq!(settings.appearance().density.indexOfSelectedItem(), 1);
        assert_eq!(
            settings
                .appearance()
                .density
                .itemTitleAtIndex(1)
                .to_string(),
            match locale {
                Locale::English => "Normal",
                Locale::Chinese => "标准",
            }
        );
        settings.appearance().density.selectItemAtIndex(1);
        let normal = settings.candidate().unwrap();
        assert_eq!(normal.display_density, DisplayDensity::Normal);
        settings.fill(&Config::from_json(&normal.to_json().unwrap()).unwrap());
        assert_eq!(settings.appearance().density.indexOfSelectedItem(), 1);
        settings.appearance().opacity_slider.setDoubleValue(67.4);
        settings.opacity_slider_changed();
        assert_eq!(
            settings
                .appearance()
                .opacity_input
                .stringValue()
                .to_string(),
            "67"
        );
        crate::macos::ui::material::tests::verify_backdrop_opacity(
            &settings.appearance().opacity_preview,
            0.67,
        );
        assert_eq!(
            settings.appearance().opacity_slider.isEnabled(),
            !crate::macos::ui::material::glass_available()
        );
        assert_eq!(
            settings.appearance().opacity_input.isEnabled(),
            !crate::macos::ui::material::glass_available()
        );
        assert_eq!(settings.candidate().unwrap().background_opacity, 67);
        for value in [0, 50, 100] {
            settings
                .appearance()
                .opacity_input
                .setStringValue(&NSString::from_str(&value.to_string()));
            settings.opacity_input_changed().unwrap();
            assert_eq!(
                settings.appearance().opacity_slider.doubleValue(),
                f64::from(value)
            );
            crate::macos::ui::material::tests::verify_backdrop_opacity(
                &settings.appearance().opacity_preview,
                f64::from(value) / 100.0,
            );
            let config = settings.candidate().unwrap();
            settings.fill(&Config::from_json(&config.to_json().unwrap()).unwrap());
            assert_eq!(settings.candidate().unwrap().background_opacity, value);
        }
        for text in ["", "-1", "101", "50.5", "abc"] {
            settings
                .appearance()
                .opacity_input
                .setStringValue(&NSString::from_str(text));
            assert!(settings.opacity_input_changed().is_err());
            assert!(settings.candidate().is_err());
        }
        settings.fill(&Config::default());
        crate::macos::ui::material::tests::verify_backdrop_opacity(
            &settings.appearance().opacity_preview,
            1.0,
        );
        verify_multiple_search_controls(&settings);
        if let Ok(directory) = std::env::var("WINLANE_PREVIEW_DIR") {
            settings.appearance().set_opacity(65);
            let view = settings.window.contentView().unwrap();
            let background = NSBox::initWithFrame(NSBox::alloc(mtm), view.bounds());
            background.setBoxType(NSBoxType::Custom);
            background.setBorderWidth(0.0);
            background.setFillColor(&NSColor::windowBackgroundColor());
            view.addSubview_positioned_relativeTo(&background, NSWindowOrderingMode::Below, None);
            for (tab, name) in [
                (1, "opacity"),
                (2, "input"),
                (3, "windows"),
                (6, "snippets"),
            ] {
                settings.select_tab(tab);
                view.layoutSubtreeIfNeeded();
                let bitmap = view
                    .bitmapImageRepForCachingDisplayInRect(view.bounds())
                    .unwrap();
                view.cacheDisplayInRect_toBitmapImageRep(view.bounds(), &bitmap);
                let png = unsafe {
                    bitmap.representationUsingType_properties(
                        NSBitmapImageFileType::PNG,
                        &objc2_foundation::NSDictionary::new(),
                    )
                }
                .unwrap();
                std::fs::create_dir_all(&directory).unwrap();
                let path = format!("{directory}/{locale:?}-{name}-settings.png");
                assert!(png.writeToFile_atomically(&NSString::from_str(&path), true));
            }
        }
        assert!(!settings.window.isVisible());
        crate::macos::ui::app_shortcuts::tests::verify_hidden_settings(target, mtm);
    }
    println!(
        "English and Chinese settings: sidebar navigation, stable window size, grouped layout bounds, configuration round trips passed."
    );
}

pub fn verify_loaded_pages(settings: &SettingsWindow, expected: &[isize]) {
    let loaded: Vec<_> = (0..settings.tabs.numberOfTabViewItems())
        .filter(|&index| !settings.host(index).subviews().is_empty())
        .collect();
    assert_eq!(loaded, expected, "only visited pages should have controls");
}

fn verify_lazy_pages(target: &AnyObject, mtm: MainThreadMarker) {
    let settings = SettingsWindow::new(target, mtm);
    let mut config = Config {
        appearance: Appearance::Dark,
        display_density: DisplayDensity::Compact,
        input_rules: winlane::features::input_rules::Settings::for_winlane(InputMethod::Chinese),
        sort: SortOrder::Title,
        excluded_apps: vec!["com.example.hidden".into()],
        ..Config::default()
    };
    config.clipboard.max_items = 37;
    config.clipboard.retention_days = 21;
    settings.fill(&config);
    settings.update_updater(true, false, None);
    verify_loaded_pages(&settings, &[]);
    assert_eq!(settings.candidate().unwrap(), config);
    settings.select_tab(4);
    verify_loaded_pages(&settings, &[4]);
    assert_eq!(settings.candidate().unwrap(), config);

    // A separate editor may update configuration before its Settings page is visited.
    config.clipboard.max_items = 83;
    settings.sync_saved_config(&config);
    settings.select_tab(7);
    verify_loaded_pages(&settings, &[4, 7]);
    assert_eq!(settings.candidate().unwrap(), config);
    settings.select_tab(1);
    let appearance_control = settings.appearance().opacity_input.clone();
    settings
        .appearance()
        .opacity_input
        .setStringValue(ns_string!("68"));
    settings.select_tab(4);
    settings.select_tab(1);
    assert_eq!(settings.appearance().opacity_input, appearance_control);
    assert_eq!(
        settings
            .appearance()
            .opacity_input
            .stringValue()
            .to_string(),
        "68"
    );
    config.background_opacity = 68;
    assert_eq!(settings.candidate().unwrap(), config);
    for index in [0, 2, 3, 5] {
        settings.select_tab(index);
    }
    verify_loaded_pages(&settings, &[0, 1, 2, 3, 4, 7]);
    assert_eq!(settings.candidate().unwrap(), config);
}

fn verify_card_appearances(mtm: MainThreadMarker) {
    let aqua = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameAqua }).unwrap();
    let dark = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }).unwrap();
    for initial in [&aqua, &dark] {
        let window = preferences_window(rect(0.0, 0.0, 180.0, 100.0), mtm);
        window.setAppearance(Some(initial));
        let root = window.contentView().unwrap();
        let backing = NSBox::initWithFrame(NSBox::alloc(mtm), root.bounds());
        backing.setBoxType(NSBoxType::Custom);
        backing.setBorderWidth(0.0);
        backing.setFillColor(&NSColor::windowBackgroundColor());
        root.addSubview(&backing);
        initial.performAsCurrentDrawingAppearance(&block2::RcBlock::new(|| {
            card(&root, rect(10.0, 10.0, 160.0, 80.0), mtm);
        }));
        for (appearance, is_dark) in [(&dark, true), (&aqua, false), (&dark, true)] {
            window.setAppearance(Some(appearance));
            root.layoutSubtreeIfNeeded();
            let bitmap = root
                .bitmapImageRepForCachingDisplayInRect(root.bounds())
                .unwrap();
            root.cacheDisplayInRect_toBitmapImageRep(root.bounds(), &bitmap);
            let pixel = bitmap
                .colorAtX_y(bitmap.pixelsWide() / 2, bitmap.pixelsHigh() / 2)
                .unwrap()
                .colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())
                .unwrap();
            let brightness =
                (pixel.redComponent() + pixel.greenComponent() + pixel.blueComponent()) / 3.0;
            assert!(
                if is_dark {
                    brightness < 0.4
                } else {
                    brightness > 0.75
                },
                "settings cards must follow the current appearance, including after toggling: dark={is_dark}, brightness={brightness}"
            );
        }
        assert!(!window.isVisible());
        window.close();
    }
}

pub fn verify_autosave_controls(settings: &SettingsWindow, saved: impl Fn() -> Config) {
    let send = |control: &NSControl| unsafe {
        assert!(control.sendAction_to(control.action(), control.target().as_deref()));
    };
    for (index, density) in [(0, DisplayDensity::Compact), (1, DisplayDensity::Normal)] {
        settings.appearance().density.selectItemAtIndex(index);
        send(&settings.appearance().density);
        assert_eq!(saved().display_density, density);
    }
    settings
        .windows()
        .switch_delay
        .setStringValue(ns_string!("150"));
    send(&settings.windows().switch_delay);
    assert_eq!(saved().switch_delay_ms, 150);
    settings
        .windows()
        .switch_delay
        .setStringValue(ns_string!("1001"));
    send(&settings.windows().switch_delay);
    assert_eq!(saved().switch_delay_ms, 150);
    settings
        .windows()
        .switch_delay
        .setStringValue(ns_string!("150"));
    input_rules::tests::verify_autosave(settings, &saved);
    scrolling::tests::verify_autosave(settings, &saved);
    files::tests::verify_autosave(settings, &saved);
    auto_cleanup::tests::verify_autosave(settings, &saved);
    settings
        .windows()
        .minimized
        .setState(NSControlStateValueOff);
    send(&settings.windows().minimized);
    assert!(!saved().include_minimized);
    for (state, show) in [
        (NSControlStateValueOff, false),
        (NSControlStateValueOn, true),
    ] {
        settings.appearance().usage_hints.setState(state);
        send(&settings.appearance().usage_hints);
        assert_eq!(saved().show_usage_hints, show);
    }
    settings.windows().sort.selectItemAtIndex(1);
    send(&settings.windows().sort);
    assert_eq!(saved().sort, SortOrder::Application);
    settings
        .windows()
        .excluded
        .setStringValue(ns_string!("Browser, Editor"));
    send(&settings.windows().excluded);
    assert_eq!(saved().excluded_apps, ["Browser", "Editor"]);
    settings.appearance().opacity_slider.setDoubleValue(61.2);
    send(&settings.appearance().opacity_slider);
    assert_eq!(saved().background_opacity, 61);
    settings
        .appearance()
        .opacity_input
        .setStringValue(ns_string!("24"));
    send(&settings.appearance().opacity_input);
    assert_eq!(saved().background_opacity, 24);
    let valid = saved();
    settings
        .appearance()
        .opacity_input
        .setStringValue(ns_string!("101"));
    send(&settings.appearance().opacity_input);
    assert_eq!(saved(), valid);
    assert!(
        settings
            .message
            .stringValue()
            .to_string()
            .starts_with("Not saved:")
    );
    settings
        .appearance()
        .opacity_input
        .setStringValue(ns_string!("18"));
    send(&settings.appearance().opacity_input);
    assert_eq!(saved().background_opacity, 18);
    let search = Shortcut {
        control: false,
        option: false,
        shift: false,
        command: true,
        key: "Space".into(),
    };
    let valid = saved();
    settings.shortcuts().search_shortcut.fill(&search);
    assert_eq!(
        settings.candidate().unwrap().shortcut,
        search,
        "Command + Space must be accepted from the native controls"
    );
    settings.fill(&valid);
    settings
        .shortcuts()
        .search_shortcut
        .fill(&valid.switch_shortcut);
    settings.shortcuts().search_shortcut.notify_changed();
    assert_eq!(
        saved(),
        valid,
        "shortcut conflicts must not overwrite the last valid config"
    );
    assert!(
        settings
            .message
            .stringValue()
            .to_string()
            .starts_with("Not saved:")
    );
    settings.fill(&valid);
    assert_eq!(
        saved(),
        valid,
        "filling controls must not emit change actions"
    );
}

fn preference_window_ids(mtm: MainThreadMarker) -> std::collections::BTreeSet<usize> {
    // NSApplication also includes AppKit's private, offscreen windows.
    NSApplication::sharedApplication(mtm)
        .windows()
        .iter()
        .filter(|window| window.downcast_ref::<PreferencesWindow>().is_some())
        .map(|window| Retained::as_ptr(&window) as usize)
        .collect()
}

fn verify_multiple_search_controls(settings: &SettingsWindow) {
    use objc2::AnyThread;
    let before = preference_window_ids(settings.window.mtm());
    let mut config = Config {
        shortcut: Shortcut {
            control: false,
            command: true,
            key: "Space".into(),
            ..Shortcut::default()
        },
        additional_search_shortcuts: vec![Shortcut::default()],
        ..Config::default()
    };
    settings.fill(&config);
    assert_eq!(settings.candidate().unwrap(), config);
    assert_eq!(
        settings.shortcuts().add_search.action(),
        Some(sel!(addSearchShortcut:))
    );
    assert_eq!(
        settings.shortcuts().search_rows.borrow()[0]
            .shortcut
            .key
            .action(),
        Some(sel!(settingsChanged:))
    );
    assert_eq!(
        settings.shortcuts().search_rows.borrow()[0].remove.action(),
        Some(sel!(removeSearchShortcut:))
    );
    let domain = NSString::from_str(&format!(
        "com.example.winlane-search-bindings-{}",
        std::process::id()
    ));
    let store = NSUserDefaults::initWithSuiteName(NSUserDefaults::alloc(), Some(&domain)).unwrap();
    store.removePersistentDomainForName(&domain);
    save(&config, &store).unwrap();
    let saved = || {
        Config::from_json(
            &store
                .stringForKey(ns_string!("WindowlanePreferencesV1"))
                .unwrap()
                .to_string(),
        )
        .unwrap()
    };
    settings.add_search_shortcut();
    assert_eq!(settings.shortcuts().search_rows.borrow().len(), 2);
    assert!(
        settings
            .candidate()
            .and_then(|candidate| save(&candidate, &store))
            .is_err()
    );
    assert_eq!(
        saved(),
        config,
        "unfinished rows must not replace saved bindings"
    );
    assert!(!settings.window.isVisible());
    assert_eq!(preference_window_ids(settings.window.mtm()), before);
    settings.shortcuts().search_rows.borrow()[1]
        .shortcut
        .fill(&config.shortcut);
    assert!(
        settings
            .candidate()
            .and_then(|candidate| save(&candidate, &store))
            .is_err()
    );
    assert_eq!(
        saved(),
        config,
        "duplicate draft must leave existing shortcuts active"
    );
    let extra = Shortcut {
        key: "KeyU".into(),
        ..Shortcut::default()
    };
    settings.shortcuts().search_rows.borrow()[1]
        .shortcut
        .fill(&extra);
    let candidate = settings.candidate().unwrap();
    save(&candidate, &store).unwrap();
    config.additional_search_shortcuts.push(extra);
    assert_eq!(saved(), config);
    settings.remove_search_shortcut(0);
    let candidate = settings.candidate().unwrap();
    save(&candidate, &store).unwrap();
    config.additional_search_shortcuts.remove(0);
    assert_eq!(saved(), config);
    assert_eq!(settings.shortcuts().search_rows.borrow()[0].remove.tag(), 0);
    config.additional_search_shortcuts = (1..winlane::core::config::MAX_SEARCH_SHORTCUTS)
        .map(|n| Shortcut {
            key: format!("F{n}"),
            ..Shortcut::default()
        })
        .collect();
    settings.fill(&config);
    assert_eq!(settings.candidate().unwrap(), config);
    assert!(!settings.shortcuts().add_search.isEnabled());
    settings.add_search_shortcut();
    assert_eq!(settings.shortcuts().search_rows.borrow().len(), 7);
    let mut previous = settings.shortcuts().search_header.frame().origin.y;
    for row in settings.shortcuts().search_rows.borrow().iter() {
        let frame = row.view.frame();
        assert!(frame.origin.y >= 0.0);
        assert!(frame.origin.y + frame.size.height <= previous);
        previous = frame.origin.y;
        for child in row.view.subviews() {
            let frame = child.frame();
            assert!(frame.origin.x >= 0.0 && frame.origin.y >= 0.0);
            assert!(frame.origin.x + frame.size.width <= row.view.bounds().size.width);
            assert!(frame.origin.y + frame.size.height <= row.view.bounds().size.height);
        }
    }
    assert!(
        settings.shortcuts().shortcuts_document.frame().size.height
            > settings.shortcuts().shortcuts_scroll.contentSize().height
    );
    assert!(settings.shortcuts().app_shortcuts_card.frame().origin.y >= 0.0);
    assert!(
        settings.shortcuts().app_shortcuts_card.frame().origin.y
            + settings.shortcuts().app_shortcuts_card.frame().size.height
            < settings.shortcuts().switch_card.frame().origin.y
    );
    assert!(
        settings.shortcuts().switch_card.frame().origin.y
            + settings.shortcuts().switch_card.frame().size.height
            < settings.shortcuts().search_card.frame().origin.y
    );
    settings.remove_search_shortcut(6);
    assert!(settings.shortcuts().add_search.isEnabled());
    settings.fill(&Config::default());
    assert!(settings.shortcuts().search_rows.borrow().is_empty());
    assert_eq!(settings.candidate().unwrap(), Config::default());
    store.removePersistentDomainForName(&domain);
}

fn verify_settings_bounds(parent: &NSView) {
    for child in parent.subviews() {
        let frame = child.frame();
        assert!(
            frame.origin.x >= 0.0 && frame.origin.y >= 0.0,
            "negative control origin: {frame:?}"
        );
        assert!(
            frame.origin.x + frame.size.width <= parent.bounds().size.width + 0.5,
            "control wider than its container: {frame:?}"
        );
        assert!(
            frame.origin.y + frame.size.height <= parent.bounds().size.height + 0.5,
            "control taller than its container: {frame:?}"
        );
        if child.downcast_ref::<NSControl>().is_none()
            && child.downcast_ref::<NSBox>().is_none()
            && child.downcast_ref::<NSScrollView>().is_none()
        {
            verify_settings_bounds(&child);
        }
    }
}

pub fn verify_sidebar_actions(settings: &SettingsWindow, saved: impl Fn() -> Config) {
    let windows = preference_window_ids(settings.window.mtm());
    assert!(windows.contains(&(Retained::as_ptr(&settings.window) as usize)));
    // Text input can lazily create an AppKit panel on some macOS versions.
    // Keep an unrelated hidden panel alive to exercise that case on every runner.
    let auxiliary = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(settings.window.mtm()),
        rect(0.0, 0.0, 40.0, 40.0),
        NSWindowStyleMask::Borderless,
        NSBackingStoreType::Buffered,
        false,
    );
    // SAFETY: The retained test owner releases the panel after closing it.
    unsafe {
        auxiliary.setReleasedWhenClosed(false);
    }
    for button in &settings.navigation {
        unsafe {
            assert!(button.sendAction_to(button.action(), button.target().as_deref()));
        }
        assert_eq!(settings.selected_tab(), button.tag());
        assert_eq!(settings.tabs.window().as_ref(), Some(&settings.window));
        assert_eq!(
            preference_window_ids(settings.window.mtm()),
            windows,
            "switching sections must reuse the existing preferences windows"
        );
    }
    settings.select_tab(3);
    unsafe { settings.windows().excluded.selectText(None) };
    let editor = settings
        .window
        .firstResponder()
        .unwrap()
        .downcast::<NSTextView>()
        .unwrap();
    editor.setString(ns_string!("Example Browser"));
    let button = &settings.navigation[0];
    unsafe {
        assert!(button.sendAction_to(button.action(), button.target().as_deref()));
    }
    assert_eq!(settings.selected_tab(), 4);
    assert_eq!(
        saved().excluded_apps,
        ["Example Browser"],
        "leaving a section must save its active text edit"
    );
    assert_eq!(preference_window_ids(settings.window.mtm()), windows);
    assert!(!settings.window.isVisible());
    assert!(!auxiliary.isVisible());
    auxiliary.close();
}

pub(crate) fn escape_event(window: &NSWindow, flags: NSEventModifierFlags) -> Retained<NSEvent> {
    NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
        NSEventType::KeyDown,
        NSPoint::new(0.0, 0.0),
        flags,
        0.0,
        window.windowNumber(),
        None,
        ns_string!("\u{1b}"),
        ns_string!("\u{1b}"),
        false,
        53,
    )
    .unwrap()
}

pub fn verify_escape_close(window: &NSWindow) {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let closed = Arc::new(AtomicUsize::new(0));
    let count = closed.clone();
    let block = block2::RcBlock::new(
        move |_: std::ptr::NonNull<objc2_foundation::NSNotification>| {
            count.fetch_add(1, Ordering::Relaxed);
        },
    );
    let center = objc2_foundation::NSNotificationCenter::defaultCenter();
    // SAFETY: The callback only touches an atomic and observes this retained window.
    let observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWindowWillCloseNotification),
            Some(window),
            None,
            &block,
        )
    };
    window.sendEvent(&escape_event(window, NSEventModifierFlags::Command));
    assert_eq!(
        closed.load(Ordering::Relaxed),
        0,
        "modified Escape must not close settings"
    );
    let mtm = MainThreadMarker::new().unwrap();
    let editor = NSTextView::initWithFrame(NSTextView::alloc(mtm), rect(0.0, 0.0, 100.0, 24.0));
    window.contentView().unwrap().addSubview(&editor);
    assert!(window.makeFirstResponder(Some(&editor)));
    // SAFETY: A native text input client accepts NSString and valid UTF-16 ranges.
    unsafe {
        NSTextInputClient::setMarkedText_selectedRange_replacementRange(
            &*editor,
            ns_string!("zhong"),
            objc2_foundation::NSRange::new(5, 0),
            objc2_foundation::NSRange::new(objc2_foundation::NSNotFound as usize, 0),
        );
    }
    assert!(NSTextInputClient::hasMarkedText(&*editor));
    window.sendEvent(&escape_event(window, NSEventModifierFlags::empty()));
    assert_eq!(
        closed.load(Ordering::Relaxed),
        0,
        "IME cancellation must not close settings"
    );
    NSTextInputClient::unmarkText(&*editor);
    assert!(window.makeFirstResponder(None));
    editor.removeFromSuperview();
    window.sendEvent(&escape_event(window, NSEventModifierFlags::empty()));
    assert_eq!(
        closed.load(Ordering::Relaxed),
        1,
        "Escape must close {}",
        window.title()
    );
    assert!(!window.isVisible());
    unsafe { center.removeObserver((*observer).as_ref()) };
}

pub fn verify_escape_autosave(settings: &SettingsWindow, saved: impl Fn() -> Config) {
    settings.select_tab(3);
    unsafe { settings.windows().excluded.selectText(None) };
    let editor = settings
        .window
        .firstResponder()
        .unwrap()
        .downcast::<NSTextView>()
        .unwrap();
    editor.setString(ns_string!("Example Browser, Example Editor"));
    settings.window.sendEvent(&escape_event(
        &settings.window,
        NSEventModifierFlags::empty(),
    ));
    assert_eq!(saved().excluded_apps, ["Example Browser", "Example Editor"]);
}
