use super::*;

pub fn verify_reveal_positions() {
    for (x, y, width, height) in [
        (0.0, 0.0, 1728.0, 1117.0),
        (-2560.0, 0.0, 2560.0, 1440.0),
        (0.0, -1440.0, 2560.0, 1440.0),
    ] {
        let point = reveal_position(NSRect::new(
            NSPoint::new(x, y),
            objc2_foundation::NSSize::new(width, height),
        ));
        assert_eq!(point.x, x + width * 0.75);
        assert_eq!(point.y, y + 1.0);
        assert!(point.x > x && point.x < x + width);
        assert!(point.y >= y && point.y < y + height);
    }
}
