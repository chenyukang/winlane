use super::*;

pub(crate) fn verify(mtm: MainThreadMarker) {
    let app = NSApplication::sharedApplication(mtm);
    let focused = app.keyWindow();
    let frontmost = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|app| app.processIdentifier());
    let mut indicator = Indicator::default();
    assert!(indicator.surfaces.is_empty());
    let mut displays = vec![
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
    let status = IndicatorStatus {
        display: false,
        remaining_minutes: Some(30),
    };
    indicator.configure(status, &displays, &[], mtm);
    assert_eq!(indicator.surfaces.len(), 2);
    let panel = indicator.surfaces[0].panel.clone();
    for surface in &indicator.surfaces {
        assert!(!surface.panel.canBecomeKeyWindow());
        assert!(!surface.panel.canBecomeMainWindow());
        assert!(surface.panel.ignoresMouseEvents());
        assert!(!surface.panel.hidesOnDeactivate());
        assert!(surface.panel.collectionBehavior().contains(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
        ));
        assert!(!surface.panel.isVisible());
        assert_eq!(surface.text.stringValue().to_string(), status.label());
        let text = surface.text.frame();
        let bounds = surface.panel.contentView().unwrap().bounds();
        assert!(text.origin.y >= 0.0 && text.origin.y + text.size.height <= bounds.size.height);
        assert!(text.origin.x >= 0.0 && text.origin.x + text.size.width <= bounds.size.width);
    }
    let original = panel.frame();
    let occupied = [Rect {
        x: original.origin.x,
        y: original.origin.y,
        width: original.size.width,
        height: original.size.height,
    }];
    let next = IndicatorStatus {
        display: true,
        remaining_minutes: None,
    };
    indicator.configure(next, &displays, &occupied, mtm);
    assert_eq!(indicator.status, Some(next));
    assert!(
        std::ptr::eq(&*indicator.surfaces[0].panel, &*panel),
        "reuse panels across mode/countdown changes"
    );
    assert_eq!(
        indicator.surfaces[0].text.stringValue().to_string(),
        next.label()
    );
    assert!(panel.frame().origin.y + panel.frame().size.height < original.origin.y);
    indicator.configure(next, &displays, &[], mtm);
    assert_eq!(
        panel.frame().origin.y,
        original.origin.y,
        "restore the corner when input indicator moves away"
    );
    displays[1].frame = displays[0].frame;
    displays[1].safe_area = displays[0].safe_area;
    indicator.configure(next, &displays, &[], mtm);
    assert_eq!(
        indicator.surfaces.len(),
        1,
        "mirrored displays need only one badge"
    );
    indicator.configure(next, &[], &[], mtm);
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
        "Keep Awake indicator: countdown and mode labels, click-through panels, safe placement, input indicator avoidance, display changes and cleanup; no overlays shown."
    );
}
