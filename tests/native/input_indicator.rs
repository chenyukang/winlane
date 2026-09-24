use super::*;
use winlane::features::input_indicator::{DisplayTarget, Position, Size};

pub(crate) fn verify(mtm: MainThreadMarker) {
    let source_before =
        crate::macos::platform::input_source::Source::current(mtm).and_then(|s| s.id());
    let frontmost_before = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|app| app.processIdentifier());
    let mut indicator = Indicator::default();
    let displays = [
        Screen {
            id: 1,
            frame: Rect {
                x: 0.0,
                y: 0.0,
                width: 1440.0,
                height: 900.0,
            },
            safe_area: Rect {
                x: 0.0,
                y: 40.0,
                width: 1440.0,
                height: 830.0,
            },
        },
        Screen {
            id: 2,
            frame: Rect {
                x: -1920.0,
                y: -200.0,
                width: 1920.0,
                height: 1080.0,
            },
            safe_area: Rect {
                x: -1920.0,
                y: -160.0,
                width: 1920.0,
                height: 1010.0,
            },
        },
    ];
    let english = InputSource {
        id: "example.english".into(),
        name: "ABC".into(),
        language: Some("en".into()),
    };
    let chinese = InputSource {
        id: "example.chinese".into(),
        name: "中文输入法".into(),
        language: Some("zh-Hans".into()),
    };
    let mut settings = Settings::default();
    indicator.configure(&settings, Some(&english), &displays, mtm);
    assert!(indicator.surfaces.is_empty());
    settings.enabled = true;
    indicator.configure(&settings, Some(&english), &displays, mtm);
    assert_eq!(indicator.surfaces.len(), 2);
    let panel = indicator.surfaces[0].panel.clone();
    for surface in &indicator.surfaces {
        assert!(!surface.panel.canBecomeKeyWindow());
        assert!(!surface.panel.canBecomeMainWindow());
        assert!(surface.panel.ignoresMouseEvents());
        assert!(!surface.panel.hidesOnDeactivate());
        assert!(!surface.panel.hasShadow());
        assert!(surface.panel.collectionBehavior().contains(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
        ));
        assert!(surface.panel.level() > NSStatusWindowLevel);
        assert!(surface.text.isHidden());
        assert!(!surface.panel.isVisible());
        assert_eq!(surface.panel.frame().size.height, 4.0);
    }
    for style in Style::ALL {
        settings.style = style;
        settings.size = Size::Large;
        settings
            .colors
            .insert(chinese.id.clone(), Color(250, 230, 90));
        for position in Position::ALL {
            settings.position = position;
            indicator.configure(&settings, Some(&chinese), &displays, mtm);
            let surface = &indicator.surfaces[0];
            assert!(
                std::ptr::eq(&*panel, &*surface.panel),
                "source and style changes reuse the panel"
            );
            assert_eq!(surface.text.stringValue().to_string(), chinese.name);
            assert_eq!(surface.text.isHidden(), style != Style::Badge);
            let frame = surface.panel.frame();
            if style == Style::Circle {
                assert_eq!(frame.size.width, f64::from(settings.shape_width));
                assert_eq!(frame.size.height, frame.size.width);
                assert_eq!(surface.background.cornerRadius(), frame.size.width / 2.0);
            } else if style == Style::RoundedRectangle {
                assert_eq!(frame.size.width, f64::from(settings.shape_width));
                assert_eq!(frame.size.height, f64::from(settings.shape_height));
                assert_eq!(
                    surface.background.cornerRadius(),
                    frame.size.height.min(frame.size.width) / 4.0
                );
            }
            let color = surface
                .background
                .fillColor()
                .colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())
                .unwrap();
            assert!((color.redComponent() - 250.0 / 255.0).abs() < 0.001);
            assert_eq!(surface.text.textColor().unwrap(), NSColor::blackColor());
            if style == Style::Badge {
                let frame = surface.text.frame();
                assert!(frame.origin.x >= 0.0 && frame.origin.y >= 0.0);
                assert!(frame.origin.y + frame.size.height <= surface.panel.frame().size.height);
            }
        }
    }
    for style in Style::ALL {
        settings.style = style;
        settings.hidden_sources.insert(english.id.clone());
        indicator.configure(&settings, Some(&english), &displays, mtm);
        assert!(
            indicator.surfaces.is_empty(),
            "hidden sources remove all display surfaces"
        );
        indicator.configure(&settings, Some(&chinese), &displays, mtm);
        assert_eq!(indicator.surfaces.len(), 2, "other sources stay visible");
        settings.hidden_sources.remove(&english.id);
        indicator.configure(&settings, Some(&english), &displays, mtm);
        assert_eq!(
            indicator.surfaces.len(),
            2,
            "re-enabling a source restores its indicator"
        );
    }
    settings.style = Style::Circle;
    settings.position = Position::TopLeft;
    settings.shape_width = 31;
    settings.offset_x = 123;
    settings.offset_y = 45;
    indicator.configure(&settings, Some(&chinese), &displays, mtm);
    for surface in &indicator.surfaces {
        let screen = displays
            .iter()
            .find(|screen| screen.id == surface.id)
            .unwrap();
        assert_eq!(
            from_rect(surface.panel.frame()),
            settings.frame(screen.frame, screen.safe_area, 0.0)
        );
        assert_eq!(surface.background.cornerRadius(), 15.5);
    }
    settings.display_target = DisplayTarget::MainDisplay;
    indicator.configure(&settings, Some(&english), &displays, mtm);
    assert_eq!(indicator.surfaces.len(), 1);
    assert_eq!(indicator.surfaces[0].id, 1);
    settings.display_target = DisplayTarget::AllDisplays;
    indicator.configure(&settings, Some(&english), &displays[1..], mtm);
    assert_eq!(indicator.surfaces.len(), 1);
    assert_eq!(indicator.surfaces[0].id, 2);
    assert!(!panel.isVisible());
    let mirrored = [
        Screen {
            id: 1,
            frame: displays[0].frame,
            safe_area: displays[0].safe_area,
        },
        Screen {
            id: 2,
            frame: displays[0].frame,
            safe_area: displays[0].safe_area,
        },
    ];
    indicator.configure(&settings, Some(&english), &mirrored, mtm);
    assert_eq!(
        indicator.surfaces.len(),
        1,
        "mirrored displays share one indicator"
    );
    indicator.configure(&settings, Some(&english), &[], mtm);
    assert!(
        indicator.surfaces.is_empty(),
        "disconnecting displays releases surfaces"
    );
    indicator.configure(&settings, None, &displays, mtm);
    assert!(
        indicator.surfaces.is_empty(),
        "unknown sources must not display stale colors"
    );
    indicator.configure(&settings, Some(&chinese), &displays, mtm);
    settings.enabled = false;
    indicator.configure(&settings, Some(&chinese), &displays, mtm);
    assert!(indicator.surfaces.is_empty());
    let enabled = crate::macos::platform::input_source::Source::enabled(mtm);
    assert!(
        enabled
            .iter()
            .all(|source| !source.id.is_empty() && !source.name.is_empty())
    );
    assert_eq!(
        enabled
            .iter()
            .map(|s| &s.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        enabled.len()
    );
    assert_eq!(
        source_before,
        crate::macos::platform::input_source::Source::current(mtm).and_then(|s| s.id())
    );
    assert_eq!(
        frontmost_before,
        NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|app| app.processIdentifier())
    );
    println!(
        "Input indicator: all shapes, dimensions, offsets, source colors, click-through nonactivating windows, multiple displays, cleanup; no overlays shown or input sources changed."
    );
}
