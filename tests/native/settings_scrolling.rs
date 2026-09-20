use super::*;
use objc2_foundation::ns_string;

pub fn verify(target: &AnyObject, mtm: MainThreadMarker) {
    let settings = SettingsWindow::new(target, mtm);
    assert!(settings.scrolling.get().is_none());
    let mut config = Config::default();
    config.scrolling.wheel_step = 3;
    settings.fill(&config);
    assert_eq!(settings.candidate().unwrap(), config);
    settings.update_scrolling_status("Scroll monitoring stopped", true);
    settings.select_tab(10);
    let page = settings.scrolling();
    assert_eq!(
        page.status.stringValue().to_string(),
        "Scroll monitoring stopped"
    );
    assert_eq!(settings.candidate().unwrap(), config);
    page.enabled.setState(NSControlStateValueOn);
    page.trackpad_horizontal.setState(NSControlStateValueOn);
    config.scrolling.enabled = true;
    config.scrolling.trackpad_horizontal = true;
    assert_eq!(settings.candidate().unwrap(), config);
    for invalid in ["-1", "101", "3.5", "bad"] {
        page.step.setStringValue(ns_string!("0"));
        assert!(settings.candidate().is_ok());
        page.step.setStringValue(&NSString::from_str(invalid));
        assert!(settings.candidate().is_err());
    }
    settings.fill(&config);
    for control in [
        &*page.enabled as &NSControl,
        &*page.mouse_vertical,
        &*page.mouse_horizontal,
        &*page.trackpad_vertical,
        &*page.trackpad_horizontal,
        &*page.step,
    ] {
        assert_eq!(control.action(), Some(sel!(settingsChanged:)));
    }
    assert!(page.step.cell().unwrap().sendsActionOnEndEditing());
    assert_eq!(settings.candidate().unwrap(), config);
    assert!(!settings.window.isVisible());
    settings.window.close();
}

pub fn verify_autosave(settings: &SettingsWindow, saved: impl Fn() -> Config) {
    let page = settings.scrolling();
    assert!(!saved().scrolling.enabled);
    let send = |control: &NSControl| unsafe {
        assert!(control.sendAction_to(control.action(), control.target().as_deref()));
    };
    page.mouse_horizontal.setState(NSControlStateValueOn);
    send(&page.mouse_horizontal);
    assert!(saved().scrolling.mouse_horizontal);
    page.step.setStringValue(ns_string!("3"));
    send(&page.step);
    let valid = saved();
    assert_eq!(valid.scrolling.wheel_step, 3);
    page.step.setStringValue(ns_string!("101"));
    send(&page.step);
    assert_eq!(saved(), valid);
    page.step.setStringValue(ns_string!("3"));
    assert!(!saved().scrolling.enabled);
}
