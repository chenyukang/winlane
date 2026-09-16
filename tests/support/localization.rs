pub fn verify_localized_settings(target: &AnyObject, mtm: MainThreadMarker) {
    use winlane::i18n::Locale;
    for (locale, title, tabs) in [
        (
            Locale::English,
            "Winlane Settings",
            [
                "Shortcuts",
                "Appearance & Language",
                "Input",
                "Window List",
                "Startup",
            ],
        ),
        (
            Locale::Chinese,
            "Winlane 设置",
            ["快捷键", "外观与语言", "输入", "窗口列表", "启动"],
        ),
    ] {
        i18n::set_locale(locale);
        let settings = SettingsWindow::new(target, mtm);
        for control in [
            &*settings.language,
            &*settings.appearance,
            &*settings.sort,
            &*settings.input_method,
        ] {
            assert_eq!(control.action(), Some(sel!(settingsChanged:)));
        }
        for shortcut in [&settings.search_shortcut, &settings.switch_shortcut] {
            assert_eq!(shortcut.key.action(), Some(sel!(settingsChanged:)));
            assert!(
                shortcut
                    .modifiers
                    .iter()
                    .all(|control| control.action() == Some(sel!(settingsChanged:)))
            );
        }
        assert!(settings.excluded.cell().unwrap().sendsActionOnEndEditing());
        assert!(
            settings
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
        assert_eq!(settings.window.title().to_string(), title);
        for (index, title) in tabs.iter().enumerate() {
            settings.select_tab(index as isize);
            let item = settings.tabs.selectedTabViewItem().unwrap();
            assert_eq!(item.label().to_string(), *title);
            assert_eq!(settings.selected_tab(), index as isize);
            settings
                .window
                .contentView()
                .unwrap()
                .layoutSubtreeIfNeeded();
            let view = item.view(mtm).unwrap();
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
        for (index, language) in [Language::System, Language::Chinese, Language::English]
            .into_iter()
            .enumerate()
        {
            let config = Config {
                language,
                excluded_apps: vec!["Example".into()],
                appearance: Appearance::Dark,
                ..Config::default()
            };
            settings.fill(&config);
            assert_eq!(settings.candidate().unwrap(), config);
            assert_eq!(settings.language.indexOfSelectedItem(), index as isize);
        }
        settings.language.selectItemAtIndex(0);
        assert_eq!(settings.candidate().unwrap().language, Language::System);
        for (index, input_method) in [
            InputMethod::Current,
            InputMethod::English,
            InputMethod::Chinese,
            InputMethod::LastUsed,
        ]
        .into_iter()
        .enumerate()
        {
            let config = Config {
                input_method,
                ..Config::default()
            };
            settings.fill(&config);
            assert_eq!(settings.input_method.indexOfSelectedItem(), index as isize);
            assert_eq!(settings.candidate().unwrap(), config);
        }
        settings.fill(&Config::default());
        assert_eq!(settings.input_method.indexOfSelectedItem(), 0);
        settings.opacity_slider.setDoubleValue(67.4);
        settings.opacity_slider_changed();
        assert_eq!(settings.opacity_input.stringValue().to_string(), "67");
        assert_eq!(settings.opacity_preview.alphaValue(), 0.67);
        assert_eq!(settings.candidate().unwrap().background_opacity, 67);
        for value in [0, 50, 100] {
            settings
                .opacity_input
                .setStringValue(&NSString::from_str(&value.to_string()));
            settings.opacity_input_changed().unwrap();
            assert_eq!(settings.opacity_slider.doubleValue(), f64::from(value));
            assert_eq!(
                settings.opacity_preview.alphaValue(),
                f64::from(value) / 100.0
            );
            let config = settings.candidate().unwrap();
            settings.fill(&Config::from_json(&config.to_json().unwrap()).unwrap());
            assert_eq!(settings.candidate().unwrap().background_opacity, value);
        }
        for text in ["", "-1", "101", "50.5", "abc"] {
            settings
                .opacity_input
                .setStringValue(&NSString::from_str(text));
            assert!(settings.opacity_input_changed().is_err());
            assert!(settings.candidate().is_err());
        }
        settings.fill(&Config::default());
        assert_eq!(settings.opacity_preview.alphaValue(), 1.0);
        if let Ok(directory) = std::env::var("WINLANE_PREVIEW_DIR") {
            settings.set_opacity(65);
            let view = settings.window.contentView().unwrap();
            let background = NSBox::initWithFrame(NSBox::alloc(mtm), view.bounds());
            background.setBoxType(NSBoxType::Custom);
            background.setBorderWidth(0.0);
            background.setFillColor(&NSColor::windowBackgroundColor());
            view.addSubview_positioned_relativeTo(&background, NSWindowOrderingMode::Below, None);
            for (tab, name) in [(1, "opacity"), (2, "input")] {
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
        crate::app_shortcuts::verify_hidden_settings(target, mtm);
    }
    println!(
        "English and Chinese settings: five tabs, input and language choices, layout bounds, configuration round trips passed."
    );
}

pub fn verify_autosave_controls(settings: &SettingsWindow, saved: impl Fn() -> Config) {
    let send = |control: &NSControl| unsafe {
        assert!(control.sendAction_to(control.action(), control.target().as_deref()));
    };
    settings.input_method.selectItemAtIndex(1);
    send(&settings.input_method);
    assert_eq!(saved().input_method, InputMethod::English);
    settings.minimized.setState(NSControlStateValueOff);
    send(&settings.minimized);
    assert!(!saved().include_minimized);
    settings.sort.selectItemAtIndex(1);
    send(&settings.sort);
    assert_eq!(saved().sort, SortOrder::Application);
    settings
        .excluded
        .setStringValue(ns_string!("Browser, Editor"));
    send(&settings.excluded);
    assert_eq!(saved().excluded_apps, ["Browser", "Editor"]);
    settings.opacity_slider.setDoubleValue(61.2);
    send(&settings.opacity_slider);
    assert_eq!(saved().background_opacity, 61);
    settings.opacity_input.setStringValue(ns_string!("24"));
    send(&settings.opacity_input);
    assert_eq!(saved().background_opacity, 24);
    let valid = saved();
    settings.opacity_input.setStringValue(ns_string!("101"));
    send(&settings.opacity_input);
    assert_eq!(saved(), valid);
    assert!(
        settings
            .message
            .stringValue()
            .to_string()
            .starts_with("Not saved:")
    );
    settings.opacity_input.setStringValue(ns_string!("18"));
    send(&settings.opacity_input);
    assert_eq!(saved().background_opacity, 18);
    let valid = saved();
    settings.search_shortcut.fill(&valid.switch_shortcut);
    settings.search_shortcut.notify_changed();
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
