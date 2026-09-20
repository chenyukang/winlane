use winlane::core::config::Config;
use winlane::features::scrolling::{Classifier, Delta, Device, Settings};

#[test]
fn settings_defaults_roundtrip_and_validation() {
    let old = Config::from_json("{}").unwrap();
    assert!(!old.scrolling.enabled);
    assert_eq!(old.scrolling, Settings::default());
    let settings = Settings {
        enabled: true,
        mouse_vertical: false,
        mouse_horizontal: true,
        trackpad_vertical: true,
        trackpad_horizontal: true,
        wheel_step: 3,
    };
    let mut config = Config {
        scrolling: settings.clone(),
        ..Config::default()
    };
    assert_eq!(
        Config::from_json(&serde_json::to_string(&config).unwrap())
            .unwrap()
            .scrolling,
        settings
    );
    config.scrolling.wheel_step = 101;
    assert!(config.validate().is_err());
}

#[test]
fn device_directions_and_step_preserve_smooth_scroll() {
    let mut s = Settings::default();
    for device in [Device::Mouse, Device::Trackpad, Device::Unknown] {
        assert_eq!(s.multipliers(device, false, 1), [1, 1]);
    }
    s.enabled = true;
    s.mouse_horizontal = true;
    s.wheel_step = 3;
    assert_eq!(s.multipliers(Device::Mouse, false, 1), [-3, -1]);
    assert_eq!(s.multipliers(Device::Mouse, false, -1), [-3, -1]);
    assert_eq!(s.multipliers(Device::Mouse, false, 4), [-1, -1]);
    assert_eq!(s.multipliers(Device::Mouse, true, 1), [-1, -1]);
    assert_eq!(s.multipliers(Device::Trackpad, true, 1), [1, 1]);
    assert_eq!(s.multipliers(Device::Unknown, true, 1), [1, 1]);
    s.mouse_vertical = false;
    s.trackpad_vertical = true;
    assert_eq!(s.multipliers(Device::Trackpad, true, 1), [-1, 1]);
    assert_eq!(s.multipliers(Device::Mouse, false, 1), [3, -1]);
    assert_eq!(s.multipliers(Device::Mouse, false, i64::MIN), [1, -1]);
}

#[test]
fn touch_detection_survives_momentum_and_interleaved_wheel() {
    let mut classifier = Classifier::default();
    assert_eq!(classifier.classify(true, true, 0), Device::Unknown);
    classifier.touch(1, 0);
    assert_eq!(classifier.classify(true, false, 1), Device::Mouse);
    classifier.touch(2, 100);
    assert_eq!(classifier.classify(true, false, 101), Device::Trackpad);
    assert_eq!(classifier.classify(false, false, 120), Device::Mouse);
    assert_eq!(classifier.classify(true, true, 600), Device::Trackpad);
    assert_eq!(classifier.classify(true, true, 1200), Device::Trackpad);
    // A new Magic Mouse gesture has no multi-finger touch events.
    assert_eq!(classifier.classify(true, false, 1500), Device::Mouse);
    assert_eq!(classifier.classify(true, true, 1800), Device::Mouse);
    assert_eq!(classifier.classify(true, true, 10000), Device::Unknown);
}

#[test]
fn all_delta_representations_keep_precision_and_do_not_overflow() {
    let delta = Delta {
        lines: -1,
        points: -7,
        fixed: -0.375,
    };
    assert_eq!(
        delta.scaled(-3),
        Delta {
            lines: 3,
            points: 21,
            fixed: 1.125
        }
    );
    assert_eq!(delta.scaled(1), delta);
    assert_eq!(
        Delta {
            lines: i64::MIN,
            points: i64::MAX,
            fixed: 1.0
        }
        .scaled(-3),
        Delta {
            lines: i64::MAX,
            points: i64::MIN,
            fixed: -3.0
        }
    );
}
