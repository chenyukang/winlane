use winlane::displays::{Display, Rect, placements};

fn display(id: u32, x: f64, y: f64, width: f64, height: f64) -> Display {
    Display {
        id,
        frame: Rect {
            x,
            y,
            width,
            height,
        },
        visible: Rect {
            x: x + 10.0,
            y: y + 40.0,
            width: width - 20.0,
            height: height - 70.0,
        },
    }
}

#[test]
fn every_extended_display_has_a_panel_but_only_one_receives_keyboard() {
    let displays = [
        display(1, 0.0, 0.0, 1920.0, 1080.0),
        display(2, 1920.0, 0.0, 2560.0, 1440.0),
    ];
    let positions = placements(&displays, (660.0, 590.0), (2200.0, 600.0), None);
    assert_eq!(positions.len(), 2);
    assert_eq!(positions.iter().filter(|p| p.receives_keyboard).count(), 1);
    assert!(positions[1].receives_keyboard);
    for (screen, panel) in displays.iter().zip(&positions) {
        assert!(panel.x >= screen.visible.x);
        assert!(panel.x + 660.0 <= screen.visible.x + screen.visible.width);
        assert!(panel.y >= screen.visible.y);
        assert!(panel.y + 590.0 <= screen.visible.y + screen.visible.height);
    }
}

#[test]
fn left_and_above_displays_use_global_points_and_respect_menu_and_dock_insets() {
    let displays = [
        display(1, 0.0, 0.0, 1512.0, 982.0),
        display(2, -1920.0, 0.0, 1920.0, 1080.0),
        display(3, 0.0, 982.0, 2560.0, 1440.0),
    ];
    let positions = placements(&displays, (660.0, 590.0), (-800.0, 500.0), None);
    assert_eq!(positions.len(), 3);
    assert!(positions[1].x < 0.0);
    assert!(positions[1].receives_keyboard);
    assert!(positions[2].y > 982.0);
    assert_eq!(positions[1].x, -1290.0);
    assert!((positions[1].y - 283.6).abs() < 0.01);
}

#[test]
fn rearrangement_preserves_the_keyboard_display_by_identity() {
    let displays = [
        display(2, -1920.0, 0.0, 1920.0, 1080.0),
        display(1, 0.0, 0.0, 1512.0, 982.0),
    ];
    let positions = placements(&displays, (660.0, 590.0), (700.0, 500.0), Some(2));
    assert!(
        positions
            .iter()
            .find(|p| p.display_id == 2)
            .unwrap()
            .receives_keyboard
    );
    assert!(
        !positions
            .iter()
            .find(|p| p.display_id == 1)
            .unwrap()
            .receives_keyboard
    );
}

#[test]
fn unplugging_the_focused_display_falls_back_to_a_remaining_screen() {
    let displays = [display(1, 0.0, 0.0, 1512.0, 982.0)];
    let positions = placements(&displays, (660.0, 590.0), (-800.0, 500.0), Some(2));
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].display_id, 1);
    assert!(positions[0].receives_keyboard);
    assert!(placements(&[], (660.0, 590.0), (0.0, 0.0), Some(2)).is_empty());
}

#[test]
fn mirrored_displays_do_not_stack_duplicate_panels() {
    let first = display(1, 0.0, 0.0, 1920.0, 1080.0);
    let second = Display { id: 2, ..first };
    assert_eq!(
        placements(&[first, second, first], (660.0, 590.0), (0.0, 0.0), Some(2)).len(),
        1
    );
}

#[test]
fn a_pointer_on_a_display_boundary_selects_exactly_one_display() {
    let displays = [
        display(1, 0.0, 0.0, 1920.0, 1080.0),
        display(2, 1920.0, 0.0, 1920.0, 1080.0),
    ];
    let positions = placements(&displays, (660.0, 590.0), (1920.0, 0.0), None);
    assert!(!positions[0].receives_keyboard);
    assert!(positions[1].receives_keyboard);
}
