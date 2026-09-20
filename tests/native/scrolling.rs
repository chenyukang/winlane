use super::*;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreate(source: *const c_void) -> Event;
    fn CGEventSetType(event: Event, kind: u32);
}

pub fn verify() {
    // Synthetic events stay in memory: no event tap is installed and nothing is posted.
    for factors in [[1, 1], [-1, 1], [1, -1], [-3, -1]] {
        unsafe {
            let event = CGEventCreate(ptr::null());
            assert!(!event.is_null());
            CGEventSetType(event, SCROLL);
            let native = NSEvent::eventWithCGEvent(&*event.cast::<CGEvent>());
            assert_eq!(
                native.unwrap().r#type(),
                objc2_app_kit::NSEventType::ScrollWheel
            );
            CGEventSetIntegerValueField(event, LINES[0], -1);
            CGEventSetIntegerValueField(event, LINES[1], 2);
            CGEventSetIntegerValueField(event, POINTS[0], -7);
            CGEventSetIntegerValueField(event, POINTS[1], 13);
            CGEventSetDoubleValueField(event, FIXED[0], -0.5);
            CGEventSetDoubleValueField(event, FIXED[1], 1.25);
            CGEventSetIntegerValueField(event, 88, 1);
            CGEventSetIntegerValueField(event, 123, 2);
            let before = [0, 1].map(|axis| Delta {
                lines: CGEventGetIntegerValueField(event, LINES[axis]),
                points: CGEventGetIntegerValueField(event, POINTS[axis]),
                fixed: CGEventGetDoubleValueField(event, FIXED[axis]),
            });
            transform(event, factors, HidApi::load().as_ref());
            for axis in 0..2 {
                let expected = before[axis].scaled(factors[axis]);
                assert_eq!(
                    CGEventGetIntegerValueField(event, LINES[axis]),
                    expected.lines
                );
                assert_eq!(
                    CGEventGetIntegerValueField(event, POINTS[axis]),
                    expected.points
                );
                assert!(
                    (CGEventGetDoubleValueField(event, FIXED[axis]) - expected.fixed).abs()
                        < 0.0001
                );
            }
            assert_eq!(CGEventGetIntegerValueField(event, 88), 1);
            assert_eq!(CGEventGetIntegerValueField(event, 123), 2);
            CFRelease(event.cast_const());
        }
    }
}
