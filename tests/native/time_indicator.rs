use super::*;
use winlane::core::displays::Rect;
use winlane::features::input_indicator::{DisplayTarget, Position, Size, TimeSettings};

pub(crate) fn verify(mtm: MainThreadMarker) {
    let app = NSApplication::sharedApplication(mtm);
    let focused = app.keyWindow();
    let frontmost = NSWorkspace::sharedWorkspace()
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
    let mut settings = TimeSettings::default();
    indicator.configure(&settings, &displays, mtm);
    assert!(indicator.surfaces.is_empty());
    settings.enabled = true;
    indicator.configure(&settings, &displays, mtm);
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
        assert!(surface.text.stringValue().to_string().len() >= 4);
        let frame = surface.panel.frame();
        assert!(frame.size.width > 0.0 && frame.size.height > 0.0);
    }
    settings.position = Position::BottomLeft;
    settings.size = Size::Large;
    settings.offset_x = 18;
    settings.offset_y = 12;
    indicator.configure(&settings, &displays, mtm);
    for surface in &indicator.surfaces {
        let screen = displays
            .iter()
            .find(|screen| screen.id == surface.id)
            .unwrap();
        let frame = surface.panel.frame();
        assert!(frame.origin.x >= screen.frame.x && frame.origin.y >= screen.frame.y);
        assert!(frame.origin.x + frame.size.width <= screen.frame.x + screen.frame.width);
        assert!(frame.origin.y + frame.size.height <= screen.frame.y + screen.frame.height);
        assert_eq!(frame.size.height, settings.size.badge_height());
    }
    settings.display_target = DisplayTarget::MainDisplay;
    indicator.configure(&settings, &displays[1..], mtm);
    assert_eq!(indicator.surfaces.len(), 1);
    indicator.configure(&settings, &[], mtm);
    assert!(indicator.surfaces.is_empty());
    drop(indicator);
    assert!(!panel.isVisible());
    assert_eq!(app.keyWindow(), focused);
    assert_eq!(
        NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|app| app.processIdentifier()),
        frontmost
    );
    println!(
        "Time indicator: badge layout, size, offsets, multiple displays and cleanup; no overlays shown."
    );
}
