use super::*;

pub fn verify(expectation: &str) {
    use objc2_app_kit::NSApplication;
    use objc2_foundation::{NSObject, NSUserDefaults};

    let mtm = MainThreadMarker::new().unwrap();
    let _app = NSApplication::sharedApplication(mtm);
    let delegate = NSObject::new();
    let result = Updater::new(mtm, &delegate);
    match expectation {
        "disabled" => assert!(result.unwrap().is_none()),
        "invalid" => assert!(result.is_err(), "a broken updater must not start"),
        "enabled" => {
            let updater = result.unwrap().expect("updater must be embedded");
            assert!(updater.can_check());
            assert!(
                !updater.automatic_checks(),
                "fixture must not contact the public feed"
            );
            // SAFETY: Read public Sparkle properties on the owning thread.
            unsafe {
                let downloads: bool = msg_send![&updater.updater, automaticallyDownloadsUpdates];
                let allowed: bool = msg_send![&updater.updater, allowsAutomaticUpdates];
                let interval: f64 = msg_send![&updater.updater, updateCheckInterval];
                assert!(!downloads && !allowed);
                assert_eq!(interval, 86400.0);
            }
            updater.set_automatic_checks(true);
            assert!(updater.automatic_checks());
            let defaults = NSUserDefaults::standardUserDefaults();
            assert!(defaults.boolForKey(ns_string!("SUEnableAutomaticChecks")));
            updater.set_automatic_checks(false);
            assert!(!updater.automatic_checks());
            assert!(!defaults.boolForKey(ns_string!("SUEnableAutomaticChecks")));
        }
        _ => panic!("unknown updater expectation"),
    }
    println!("Sparkle runtime probe passed: {expectation}");
}
