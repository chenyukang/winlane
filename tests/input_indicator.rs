use winlane::core::{config::Config, displays::Rect};
use winlane::features::input_indicator::{
    Color, DisplayTarget, InputSource, Position, Settings, Size, Style,
};

#[test]
fn old_preferences_keep_indicator_disabled_and_rules_round_trip() {
    let mut config = Config::from_json(r#"{"input_method":"Chinese"}"#).unwrap();
    assert!(!config.input_indicator.enabled);
    assert!(config.input_indicator.hidden_sources.is_empty());
    config.input_indicator.enabled = true;
    config.input_indicator.style = Style::Badge;
    config.input_indicator.position = Position::BottomRight;
    config
        .input_indicator
        .colors
        .insert("example.input".into(), Color(12, 34, 56));
    config
        .input_indicator
        .hidden_sources
        .insert("example.input".into());
    assert_eq!(
        Config::from_json(&config.to_json().unwrap()).unwrap(),
        config
    );
    config.input_indicator.bar_length_percent = 0;
    assert!(config.validate().is_err());
    config.input_indicator.bar_length_percent = 100;
    config
        .input_indicator
        .colors
        .insert("".into(), Color(0, 0, 0));
    assert!(config.validate().is_err());
    config.input_indicator.colors.remove("");
    config.input_indicator.hidden_sources.insert("".into());
    assert!(config.validate().is_err());
}

#[test]
fn colors_are_saved_per_source_and_have_legible_badge_text() {
    let english = InputSource {
        id: "example.en".into(),
        name: "ABC".into(),
        language: Some("en-US".into()),
    };
    let chinese = InputSource {
        id: "example.zh".into(),
        name: "中文".into(),
        language: Some("zh-Hans".into()),
    };
    let mut settings = Settings::default();
    assert_ne!(settings.color(&english), settings.color(&chinese));
    settings
        .colors
        .insert(chinese.id.clone(), Color(20, 30, 40));
    assert_eq!(settings.color(&chinese), Color(20, 30, 40));
    let renamed = InputSource {
        name: "New Name".into(),
        ..chinese
    };
    assert_eq!(settings.color(&renamed), Color(20, 30, 40));
    assert!(Color(255, 255, 255).dark_text());
    assert!(Color(255, 230, 30).dark_text());
    assert!(!Color(0, 0, 0).dark_text());
    assert!(!Color(20, 30, 40).dark_text());
}

#[test]
fn geometry_respects_edges_safe_area_and_negative_monitor_coordinates() {
    let screen = Rect {
        x: -1920.0,
        y: -300.0,
        width: 1920.0,
        height: 1080.0,
    };
    let safe = Rect {
        x: -1920.0,
        y: -250.0,
        width: 1920.0,
        height: 990.0,
    };
    for style in Style::ALL {
        for size in [Size::Small, Size::Medium, Size::Large] {
            for position in Position::ALL {
                let settings = Settings {
                    style,
                    size,
                    position,
                    bar_length_percent: 50,
                    ..Settings::default()
                };
                let frame = settings.frame(screen, safe, 10_000.0);
                let area = if style == Style::Bar { screen } else { safe };
                assert!(frame.x >= area.x && frame.y >= area.y);
                assert!(frame.x + frame.width <= area.x + area.width);
                assert!(frame.y + frame.height <= area.y + area.height);
                assert!(frame.width > 0.0 && frame.height > 0.0);
                if style == Style::Bar {
                    match position {
                        Position::Left | Position::Right => {
                            assert_eq!(frame.width, size.thickness());
                            assert_eq!(frame.height, screen.height / 2.0);
                        }
                        _ => {
                            assert_eq!(frame.height, size.thickness());
                            assert_eq!(frame.width, screen.width / 2.0);
                        }
                    }
                } else if style == Style::Badge {
                    assert_eq!(frame.height, size.badge_height());
                    assert_eq!(frame.width, 280.0);
                } else {
                    assert_eq!(frame.width, f64::from(settings.shape_width));
                    assert_eq!(
                        frame.height,
                        f64::from(if style == Style::Circle {
                            settings.shape_width
                        } else {
                            settings.shape_height
                        })
                    );
                }
            }
        }
    }
    let top = Settings::default().frame(screen, safe, 20.0);
    assert_eq!(top.x, screen.x);
    assert_eq!(top.y + top.height, screen.y + screen.height);
    assert_eq!(top.width, screen.width);
    let tiny = Rect {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
    };
    let frame = Settings {
        style: Style::Badge,
        ..Settings::default()
    }
    .frame(tiny, tiny, 500.0);
    assert!(frame.x + frame.width <= 10.0 && frame.y + frame.height <= 10.0);
}

#[test]
fn shape_settings_migrate_validate_and_persist() {
    let old = Config::from_json(r#"{"input_indicator":{"enabled":true,"style":"Bar","size":"Large","colors":{"example.en":[12,34,56]}}}"#).unwrap();
    assert!(old.input_indicator.enabled);
    assert_eq!(old.input_indicator.size, Size::Large);
    assert_eq!(old.input_indicator.offset_x, 0);
    assert_eq!(old.input_indicator.offset_y, 0);
    for style in [Style::Circle, Style::RoundedRectangle] {
        let mut config = old.clone();
        config.input_indicator.style = style;
        config.input_indicator.shape_width = 31;
        config.input_indicator.shape_height = 17;
        config.input_indicator.offset_x = -88;
        config.input_indicator.offset_y = 47;
        assert_eq!(
            Config::from_json(&config.to_json().unwrap()).unwrap(),
            config
        );
        for value in [0, 3, 513, u16::MAX] {
            let mut invalid = config.clone();
            invalid.input_indicator.shape_width = value;
            assert!(invalid.validate().is_err());
            invalid.input_indicator.shape_width = 31;
            invalid.input_indicator.shape_height = value;
            assert!(invalid.validate().is_err());
        }
        for value in [i32::MIN, -10001, 10001, i32::MAX] {
            let mut invalid = config.clone();
            invalid.input_indicator.offset_x = value;
            assert!(invalid.validate().is_err());
            invalid.input_indicator.offset_x = 0;
            invalid.input_indicator.offset_y = value;
            assert!(invalid.validate().is_err());
        }
    }
}

#[test]
fn offsets_move_right_and_down_and_clamp_to_each_display() {
    for screen in [
        Rect {
            x: 0.0,
            y: 0.0,
            width: 1440.0,
            height: 900.0,
        },
        Rect {
            x: -1920.0,
            y: -300.0,
            width: 1920.0,
            height: 1080.0,
        },
    ] {
        let safe = Rect {
            y: screen.y + 40.0,
            height: screen.height - 70.0,
            ..screen
        };
        let mut settings = Settings {
            style: Style::Circle,
            position: Position::TopLeft,
            shape_width: 31,
            ..Settings::default()
        };
        let initial = settings.frame(screen, safe, 500.0);
        assert_eq!(initial.width, 31.0);
        assert_eq!(initial.height, 31.0);
        settings.offset_x = 123;
        settings.offset_y = 45;
        let shifted = settings.frame(screen, safe, 500.0);
        assert_eq!(shifted.x, initial.x + 123.0);
        assert_eq!(shifted.y, initial.y - 45.0);
        for style in Style::ALL {
            settings.style = style;
            for position in Position::ALL {
                settings.position = position;
                for (x, y) in [(-10000, -10000), (10000, 10000)] {
                    settings.offset_x = x;
                    settings.offset_y = y;
                    let frame = settings.frame(screen, safe, 80.0);
                    assert!(frame.x >= screen.x && frame.y >= screen.y);
                    assert!(frame.x + frame.width <= screen.x + screen.width);
                    assert!(frame.y + frame.height <= screen.y + screen.height);
                    assert_eq!(
                        frame.x,
                        if x < 0 {
                            screen.x
                        } else {
                            screen.x + screen.width - frame.width
                        }
                    );
                    assert_eq!(
                        frame.y,
                        if y > 0 {
                            screen.y
                        } else {
                            screen.y + screen.height - frame.height
                        }
                    );
                }
            }
        }
    }
}

#[test]
fn time_indicator_round_trips_and_places_badges() {
    let mut config = Config::default();
    assert!(!config.time_indicator.enabled);
    config.time_indicator.enabled = true;
    config.time_indicator.position = Position::TopRight;
    config.time_indicator.size = Size::Large;
    config.time_indicator.display_target = DisplayTarget::MainDisplay;
    config.time_indicator.color = Color(12, 34, 56);
    config.time_indicator.offset_x = -33;
    config.time_indicator.offset_y = 21;
    assert_eq!(
        Config::from_json(&config.to_json().unwrap()).unwrap(),
        config
    );
    config.time_indicator.offset_x = -10001;
    assert!(config.validate().is_err());
    config.time_indicator.offset_x = -33;
    config.time_indicator.offset_y = 10001;
    assert!(config.validate().is_err());
    let screen = Rect {
        x: -1920.0,
        y: -300.0,
        width: 1920.0,
        height: 1080.0,
    };
    let safe = Rect {
        x: -1920.0,
        y: -250.0,
        width: 1920.0,
        height: 990.0,
    };
    let frame = config.time_indicator.frame(screen, safe, 88.0);
    assert!(frame.x >= screen.x && frame.y >= screen.y);
    assert!(frame.x + frame.width <= screen.x + screen.width);
    assert!(frame.y + frame.height <= screen.y + screen.height);
    assert_eq!(frame.height, config.time_indicator.size.badge_height());
}
