pub fn verify_localized_settings(target: &AnyObject, mtm: MainThreadMarker) {
    use winlane::i18n::Locale;
    for (locale, title, tabs) in [
        (Locale::English, "Winlane Settings", ["Shortcuts", "Appearance & Language", "Window List", "Startup"]),
        (Locale::Chinese, "Winlane 设置", ["快捷键", "外观与语言", "窗口列表", "启动"]),
    ] {
        i18n::set_locale(locale);
        let settings = SettingsWindow::new(target, mtm);
        assert_eq!(settings.window.title().to_string(), title);
        for (index, title) in tabs.iter().enumerate() {
            settings.select_tab(index as isize);
            let item = settings.tabs.selectedTabViewItem().unwrap();
            assert_eq!(item.label().to_string(), *title);
            assert_eq!(settings.selected_tab(), index as isize);
            settings.window.contentView().unwrap().layoutSubtreeIfNeeded();
            let view = item.view(mtm).unwrap();
            for child in view.subviews() {
                let frame = child.frame();
                assert!(frame.origin.x >= 0.0 && frame.origin.y >= 0.0);
                assert!((frame.origin.x + frame.size.width) <= view.bounds().size.width, "control exceeds tab width: {frame:?}");
                assert!((frame.origin.y + frame.size.height) <= view.bounds().size.height, "control exceeds tab height: {frame:?}");
            }
        }
        for (index, language) in [Language::System, Language::Chinese, Language::English].into_iter().enumerate() {
            let config = Config { language, excluded_apps: vec!["Example".into()], appearance: Appearance::Dark, ..Config::default() };
            settings.fill(&config);
            assert_eq!(settings.candidate().unwrap(), config);
            assert_eq!(settings.language.indexOfSelectedItem(), index as isize);
        }
        settings.language.selectItemAtIndex(0);
        assert_eq!(settings.candidate().unwrap().language, Language::System);
        assert!(!settings.window.isVisible());
        crate::app_shortcuts::verify_hidden_settings(target, mtm);
    }
    println!("English and Chinese settings: four tabs, language choices, layout bounds, configuration round trips passed.");
}
