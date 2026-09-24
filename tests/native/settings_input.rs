use super::*;

pub(crate) fn verify(target: &AnyObject, mtm: MainThreadMarker) {
    let settings = SettingsWindow::new(target, mtm);
    let mut config = Config {
        input_rules: winlane::features::input_rules::Settings::for_winlane(InputMethod::Chinese),
        ..Config::default()
    };
    config
        .input_indicator
        .colors
        .insert("example.unavailable".into(), Color(30, 40, 50));
    config.time_indicator.enabled = true;
    config.time_indicator.position = Position::BottomLeft;
    config.time_indicator.size = Size::Large;
    config.time_indicator.display_target =
        winlane::features::input_indicator::DisplayTarget::MainDisplay;
    config.time_indicator.color = Color(15, 25, 35);
    config.time_indicator.offset_x = -12;
    config.time_indicator.offset_y = 9;
    settings.fill(&config);
    let page = settings.input();
    for control in [
        &*page.enabled as &NSControl,
        &*page.style,
        &*page.position,
        &*page.size,
        &*page.length,
        &*page.displays,
        &*page.color,
        &*page.source_visible,
        &*page.shape_width,
        &*page.shape_height,
        &*page.offset_x,
        &*page.offset_y,
        &*page.time_enabled as &NSControl,
        &*page.time_position,
        &*page.time_size,
        &*page.time_displays,
        &*page.time_color,
        &*page.time_offset_x,
        &*page.time_offset_y,
    ] {
        assert_eq!(control.action(), Some(sel!(settingsChanged:)));
    }
    assert_eq!(page.source.action(), Some(sel!(indicatorSourceSelected:)));
    assert_eq!(page.reset.action(), Some(sel!(resetIndicatorColor:)));
    assert_eq!(
        page.time_reset.action(),
        Some(sel!(resetTimeIndicatorColor:))
    );
    assert_eq!(page.color.colorWellStyle(), NSColorWellStyle::Minimal);
    assert!(!page.color.supportsAlpha());
    assert_eq!(page.time_color.colorWellStyle(), NSColorWellStyle::Minimal);
    assert!(!page.time_color.supportsAlpha());
    let sources = vec![
        InputSource {
            id: "example.english".into(),
            name: "ABC".into(),
            language: Some("en".into()),
        },
        InputSource {
            id: "example.chinese".into(),
            name: "中文输入法".into(),
            language: Some("zh-Hans".into()),
        },
    ];
    page.source.removeAllItems();
    for source in &sources {
        page.source
            .addItemWithTitle(&NSString::from_str(&source.name));
    }
    page.sources.replace(sources);
    page.source.selectItemAtIndex(0);
    settings.indicator_source_selected();
    page.enabled.setState(NSControlStateValueOn);
    page.style.selectItemAtIndex(1);
    page.position.selectItemAtIndex(5);
    page.size.selectItemAtIndex(2);
    page.length.selectItemAtIndex(1);
    page.displays.selectItemAtIndex(1);
    page.color.setColor(&native_color(Color(123, 50, 200)));
    let config = settings.candidate().unwrap();
    assert_eq!(config.input_rules.winlane_policy(), InputMethod::Chinese);
    let indicator = &config.input_indicator;
    assert!(indicator.enabled);
    assert_eq!(indicator.style, Style::Badge);
    assert_eq!(indicator.position, Position::TopRight);
    assert_eq!(indicator.size, Size::Large);
    assert_eq!(indicator.bar_length_percent, 50);
    assert_eq!(
        indicator.display_target,
        winlane::features::input_indicator::DisplayTarget::MainDisplay
    );
    assert_eq!(indicator.colors["example.english"], Color(123, 50, 200));
    assert_eq!(indicator.colors["example.unavailable"], Color(30, 40, 50));
    assert!(config.time_indicator.enabled);
    assert_eq!(config.time_indicator.position, Position::BottomLeft);
    assert_eq!(config.time_indicator.size, Size::Large);
    assert_eq!(
        config.time_indicator.display_target,
        winlane::features::input_indicator::DisplayTarget::MainDisplay
    );
    assert_eq!(config.time_indicator.color, Color(15, 25, 35));
    assert_eq!(config.time_indicator.offset_x, -12);
    assert_eq!(config.time_indicator.offset_y, 9);
    settings.sync_saved_config(&config);
    assert!(
        !page.length.isEnabled(),
        "bar length does not apply to badges"
    );

    page.source.selectItemAtIndex(1);
    settings.indicator_source_selected();
    assert_eq!(settings.candidate().unwrap(), config);
    page.color.setColor(&native_color(Color(40, 160, 170)));
    let config = settings.candidate().unwrap();
    assert_eq!(
        config.input_indicator.colors["example.english"],
        Color(123, 50, 200)
    );
    assert_eq!(
        config.input_indicator.colors["example.chinese"],
        Color(40, 160, 170)
    );
    settings.sync_saved_config(&config);
    page.source.selectItemAtIndex(0);
    settings.indicator_source_selected();
    assert_eq!(settings.candidate().unwrap(), config);
    page.source_visible.setState(NSControlStateValueOff);
    let hidden = settings.candidate().unwrap();
    assert!(
        hidden
            .input_indicator
            .hidden_sources
            .contains("example.english")
    );
    assert_eq!(hidden.input_indicator.colors, config.input_indicator.colors);
    settings.sync_saved_config(&hidden);
    assert!(!page.color.isEnabled());
    assert!(!page.reset.isEnabled());
    page.source.selectItemAtIndex(1);
    settings.indicator_source_selected();
    assert_eq!(page.source_visible.state(), NSControlStateValueOn);
    assert!(page.color.isEnabled());
    assert_eq!(settings.candidate().unwrap(), hidden);
    page.source.selectItemAtIndex(0);
    settings.indicator_source_selected();
    assert_eq!(page.source_visible.state(), NSControlStateValueOff);
    assert!(!page.color.isEnabled());
    assert_eq!(settings.candidate().unwrap(), hidden);
    let reopened = SettingsWindow::new(target, mtm);
    reopened.fill(&Config::from_json(&hidden.to_json().unwrap()).unwrap());
    reopened.select_tab(2);
    assert_eq!(
        reopened.candidate().unwrap(),
        hidden,
        "hidden source rules survive reopen"
    );
    page.source_visible.setState(NSControlStateValueOn);
    assert_eq!(
        settings.candidate().unwrap(),
        config,
        "re-enabling preserves the custom color"
    );
    settings.sync_saved_config(&config);
    assert!(page.color.isEnabled());
    settings.reset_indicator_color();
    let config = settings.candidate().unwrap();
    assert!(
        !config
            .input_indicator
            .colors
            .contains_key("example.english")
    );
    assert_eq!(
        config.input_indicator.colors["example.chinese"],
        Color(40, 160, 170)
    );
    assert_eq!(config.input_indicator.colors.len(), 2);
    page.style.selectItemAtIndex(0);
    settings.sync_saved_config(&settings.candidate().unwrap());
    assert!(page.length.isEnabled());

    page.time_enabled.setState(NSControlStateValueOff);
    let disabled_time = settings.candidate().unwrap();
    assert!(!disabled_time.time_indicator.enabled);
    settings.sync_saved_config(&disabled_time);
    assert!(!page.time_position.isEnabled());
    assert!(!page.time_color.isEnabled());
    page.time_enabled.setState(NSControlStateValueOn);
    page.time_position.selectItemAtIndex(4);
    page.time_size.selectItemAtIndex(0);
    page.time_displays.selectItemAtIndex(0);
    page.time_color.setColor(&native_color(Color(55, 160, 210)));
    page.time_offset_x.setStringValue(&NSString::from_str("14"));
    page.time_offset_y.setStringValue(&NSString::from_str("-8"));
    let time_config = settings.candidate().unwrap();
    assert!(time_config.time_indicator.enabled);
    assert_eq!(time_config.time_indicator.position, Position::TopLeft);
    assert_eq!(time_config.time_indicator.size, Size::Small);
    assert_eq!(
        time_config.time_indicator.display_target,
        winlane::features::input_indicator::DisplayTarget::AllDisplays
    );
    assert_eq!(time_config.time_indicator.color, Color(55, 160, 210));
    assert_eq!(time_config.time_indicator.offset_x, 14);
    assert_eq!(time_config.time_indicator.offset_y, -8);
    settings.sync_saved_config(&time_config);
    let config = time_config;

    let restored = SettingsWindow::new(target, mtm);
    restored.fill(&Config::from_json(&config.to_json().unwrap()).unwrap());
    assert_eq!(
        restored.candidate().unwrap(),
        config,
        "unvisited Input retains settings"
    );
    restored.select_tab(2);
    assert_eq!(
        restored.candidate().unwrap(),
        config,
        "opening Input retains unavailable source colors"
    );
    assert!(!settings.window.isVisible());
    assert!(!restored.window.isVisible());
    verify_shapes(&restored);
    println!(
        "Input indicator settings: all styles, numeric dimensions/offsets, validation, autosave, per-source visibility and colors preserve other input preferences; windows hidden."
    );
}

fn verify_shapes(settings: &SettingsWindow) {
    let page = settings.input();
    for field in [
        &page.shape_width,
        &page.shape_height,
        &page.offset_x,
        &page.offset_y,
    ] {
        assert!(field.cell().unwrap().sendsActionOnEndEditing());
    }
    for (index, style) in Style::ALL.into_iter().enumerate() {
        page.style.selectItemAtIndex(index as isize);
        page.shape_width.setStringValue(&NSString::from_str("31"));
        page.shape_height.setStringValue(&NSString::from_str("17"));
        page.offset_x.setStringValue(&NSString::from_str("-88"));
        page.offset_y.setStringValue(&NSString::from_str("47"));
        let config = settings.candidate().unwrap();
        assert_eq!(config.input_indicator.style, style);
        assert_eq!(config.input_indicator.offset_x, -88);
        assert_eq!(config.input_indicator.offset_y, 47);
        if style.is_shape() {
            assert_eq!(config.input_indicator.shape_width, 31);
        }
        if style == Style::RoundedRectangle {
            assert_eq!(config.input_indicator.shape_height, 17);
        }
        settings.sync_saved_config(&config);
        assert_eq!(page.size.isEnabled(), !style.is_shape());
        assert_eq!(page.shape_width.isEnabled(), style.is_shape());
        assert_eq!(
            page.shape_height.isEnabled(),
            style == Style::RoundedRectangle
        );
        assert_eq!(page.length.isEnabled(), style == Style::Bar);
        settings.fill(&Config::from_json(&config.to_json().unwrap()).unwrap());
        assert_eq!(settings.candidate().unwrap(), config);
    }
    let saved = settings.candidate().unwrap();
    for (field, invalid) in [
        (&page.shape_width, "0"),
        (&page.shape_height, "513"),
        (&page.offset_x, "-10001"),
        (&page.offset_y, "10001"),
        (&page.offset_y, "2147483648"),
        (&page.shape_width, "12.5"),
        (&page.offset_x, ""),
        (&page.offset_y, "abc"),
    ] {
        field.setStringValue(&NSString::from_str(invalid));
        assert!(settings.candidate().is_err());
        assert_eq!(
            *settings.config.borrow(),
            saved,
            "invalid edits keep saved preferences"
        );
        settings.fill(&saved);
    }
    page.shape_width.setStringValue(&NSString::from_str(""));
    page.style.selectItemAtIndex(0);
    let bar = settings.candidate().unwrap();
    assert_eq!(
        bar.input_indicator.shape_width, saved.input_indicator.shape_width,
        "inactive size fields keep their saved value"
    );
}
