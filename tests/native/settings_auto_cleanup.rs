use super::*;

fn application() -> ApplicationTarget {
    ApplicationTarget {
        bundle_id: "test.editor".into(),
        path: "/Applications/Editor.app".into(),
        name: "Editor".into(),
    }
}

pub fn verify(target: &AnyObject, mtm: MainThreadMarker) {
    let settings = SettingsWindow::new(target, mtm);
    assert!(settings.auto_cleanup.get().is_none());
    let mut config = Config::default();
    config.auto_cleanup.rules.push(Rule {
        application: application(),
        max_windows: 3,
    });
    settings.fill(&config);
    assert_eq!(settings.candidate().unwrap(), config);
    settings.select_tab(12);
    let page = settings.auto_cleanup();
    assert_eq!(settings.candidate().unwrap(), config);
    assert_eq!(page.interval.stringValue().to_string(), "10");
    assert!(page.grace.stringValue().to_string().contains("20"));
    assert_eq!(page.interval.action(), Some(sel!(settingsChanged:)));
    assert!(page.interval.cell().unwrap().sendsActionOnEndEditing());
    for invalid in ["0", "3601", "-1", "1.5", "bad", ""] {
        page.interval.setStringValue(&NSString::from_str(invalid));
        assert!(settings.candidate().is_err());
    }
    page.interval.setStringValue(ns_string!("30"));
    config.auto_cleanup.interval_secs = 30;
    assert_eq!(settings.candidate().unwrap(), config);
    settings.sync_saved_config(&config);
    assert!(page.grace.stringValue().to_string().contains("60"));
    assert_eq!(page.enabled.action(), Some(sel!(toggleAutoCleanup:)));
    page.enabled.setState(NSControlStateValueOn);
    config.auto_cleanup.enabled = true;
    assert_eq!(settings.candidate().unwrap(), config);
    let row = page.rows.borrow()[0].clone();
    assert_eq!(row.choose.action(), Some(sel!(chooseCleanupApp:)));
    assert_eq!(row.remove.action(), Some(sel!(removeCleanupRule:)));
    assert_eq!(row.limit.action(), Some(sel!(settingsChanged:)));
    assert!(row.limit.cell().unwrap().sendsActionOnEndEditing());
    for invalid in ["0", "101", "-1", "3.5", "bad", ""] {
        row.limit.setStringValue(&NSString::from_str(invalid));
        assert!(settings.candidate().is_err());
    }
    row.limit.setStringValue(ns_string!("3"));
    page.enabled.setState(NSControlStateValueOff);
    config.auto_cleanup.enabled = false;
    assert_eq!(
        settings.candidate().unwrap(),
        config,
        "global off preserves rules"
    );
    let draft = page.append();
    page.layout();
    assert_eq!(
        settings.candidate().unwrap(),
        config,
        "unselected app is inactive"
    );
    AutoCleanupPage::set_application(&draft, application());
    assert!(settings.candidate().is_err(), "duplicates rejected");
    page.remove(1);
    page.remove(0);
    assert!(settings.candidate().unwrap().auto_cleanup.rules.is_empty());
    assert!(!settings.window.isVisible());
    settings.window.close();
}

pub fn verify_autosave(settings: &SettingsWindow, saved: impl Fn() -> Config) {
    let page = settings.auto_cleanup();
    let row = page.append();
    AutoCleanupPage::set_application(&row, application());
    page.layout();
    let send = |control: &NSControl| unsafe {
        assert!(control.sendAction_to(control.action(), control.target().as_deref()));
    };
    page.interval.setStringValue(ns_string!("30"));
    send(&page.interval);
    assert_eq!(saved().auto_cleanup.interval_secs, 30);
    assert!(page.grace.stringValue().to_string().contains("60"));
    let valid_interval = saved();
    page.interval.setStringValue(ns_string!("0"));
    send(&page.interval);
    assert_eq!(saved(), valid_interval);
    page.interval.setStringValue(ns_string!("30"));
    send(&row.limit);
    assert_eq!(saved().auto_cleanup.rules[0].max_windows, 3);
    page.enabled.setState(NSControlStateValueOn);
    send(&page.enabled);
    assert!(saved().auto_cleanup.enabled);
    let valid = saved();
    page.interval.setStringValue(ns_string!("0"));
    row.limit.setStringValue(ns_string!("0"));
    send(&row.limit);
    assert_eq!(saved(), valid);
    page.enabled.setState(NSControlStateValueOff);
    send(&page.enabled);
    assert!(
        !saved().auto_cleanup.enabled,
        "off works with an invalid draft"
    );
    assert_eq!(saved().auto_cleanup.rules, valid.auto_cleanup.rules);
    assert_eq!(saved().auto_cleanup.interval_secs, 30);
    page.interval.setStringValue(ns_string!("30"));
    row.limit.setStringValue(ns_string!("5"));
    send(&row.limit);
    assert_eq!(saved().auto_cleanup.rules[0].max_windows, 5);
    page.enabled.setState(NSControlStateValueOff);
    send(&page.enabled);
    assert!(!saved().auto_cleanup.enabled);
    assert_eq!(saved().auto_cleanup.rules[0].max_windows, 5);
    send(&row.remove);
    assert!(saved().auto_cleanup.rules.is_empty());
    page.interval.setStringValue(ns_string!("10"));
    send(&page.interval);
}
