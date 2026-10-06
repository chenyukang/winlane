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
    assert!(settings.auto_appclose.get().is_none());
    let mut config = Config::default();
    config.auto_appclose.rules.push(Rule {
        application: application(),
        max_windows: Some(3),
        max_idle_minutes: None,
    });
    settings.fill(&config);
    assert_eq!(settings.candidate().unwrap(), config);
    settings.select_tab(12);
    let page = settings.auto_appclose();
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
    config.auto_appclose.interval_secs = 30;
    assert_eq!(settings.candidate().unwrap(), config);
    settings.sync_saved_config(&config);
    assert!(page.grace.stringValue().to_string().contains("60"));
    assert_eq!(page.enabled.action(), Some(sel!(toggleAutoAppClose:)));
    page.enabled.setState(NSControlStateValueOn);
    config.auto_appclose.enabled = true;
    assert_eq!(settings.candidate().unwrap(), config);
    let row = page.rows.borrow()[0].clone();
    assert_eq!(row.choose.action(), Some(sel!(chooseAppCloseApp:)));
    assert_eq!(row.remove.action(), Some(sel!(removeAppCloseRule:)));
    assert_eq!(row.limit.action(), Some(sel!(settingsChanged:)));
    assert!(row.limit.cell().unwrap().sendsActionOnEndEditing());
    for invalid in ["0", "101", "-1", "3.5", "bad", ""] {
        row.limit.setStringValue(&NSString::from_str(invalid));
        assert!(settings.candidate().is_err());
    }
    row.limit.setStringValue(ns_string!("3"));
    assert_eq!(row.idle.action(), Some(sel!(settingsChanged:)));
    assert!(row.idle.cell().unwrap().sendsActionOnEndEditing());
    for invalid in ["0", "10081", "-1", "1.5", "bad"] {
        row.idle.setStringValue(&NSString::from_str(invalid));
        assert!(settings.candidate().is_err());
    }
    row.idle.setStringValue(ns_string!("240"));
    config.auto_appclose.rules[0].max_idle_minutes = Some(240);
    assert_eq!(settings.candidate().unwrap(), config);
    // An idle limit works on its own: leaving the window count empty is the
    // "close Finder windows idle for 4 hours" rule.
    row.limit.setStringValue(&NSString::from_str(""));
    config.auto_appclose.rules[0].max_windows = None;
    assert_eq!(settings.candidate().unwrap(), config);
    // With neither limit the rule would do nothing.
    row.idle.setStringValue(&NSString::from_str(""));
    assert!(
        settings.candidate().is_err(),
        "a rule needs at least one limit"
    );
    row.idle.setStringValue(ns_string!("240"));
    config.auto_appclose.rules[0].max_idle_minutes = Some(240);
    assert_eq!(settings.candidate().unwrap(), config);
    row.limit.setStringValue(ns_string!("3"));
    config.auto_appclose.rules[0].max_windows = Some(3);
    page.enabled.setState(NSControlStateValueOff);
    config.auto_appclose.enabled = false;
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
    AutoAppClosePage::set_application(&draft, application());
    assert!(settings.candidate().is_err(), "duplicates rejected");
    page.remove(1);
    page.remove(0);
    assert!(settings.candidate().unwrap().auto_appclose.rules.is_empty());
    assert!(!settings.window.isVisible());
    settings.window.close();
}

pub fn verify_autosave(settings: &SettingsWindow, saved: impl Fn() -> Config) {
    let page = settings.auto_appclose();
    let row = page.append();
    AutoAppClosePage::set_application(&row, application());
    page.layout();
    let send = |control: &NSControl| unsafe {
        assert!(control.sendAction_to(control.action(), control.target().as_deref()));
    };
    page.interval.setStringValue(ns_string!("30"));
    send(&page.interval);
    assert_eq!(saved().auto_appclose.interval_secs, 30);
    assert!(page.grace.stringValue().to_string().contains("60"));
    let valid_interval = saved();
    page.interval.setStringValue(ns_string!("0"));
    send(&page.interval);
    assert_eq!(saved(), valid_interval);
    page.interval.setStringValue(ns_string!("30"));
    send(&row.limit);
    assert_eq!(saved().auto_appclose.rules[0].max_windows, Some(3));
    page.enabled.setState(NSControlStateValueOn);
    send(&page.enabled);
    assert!(saved().auto_appclose.enabled);
    let valid = saved();
    page.interval.setStringValue(ns_string!("0"));
    row.limit.setStringValue(ns_string!("0"));
    send(&row.limit);
    assert_eq!(saved(), valid);
    page.enabled.setState(NSControlStateValueOff);
    send(&page.enabled);
    assert!(
        !saved().auto_appclose.enabled,
        "off works with an invalid draft"
    );
    assert_eq!(saved().auto_appclose.rules, valid.auto_appclose.rules);
    assert_eq!(saved().auto_appclose.interval_secs, 30);
    page.interval.setStringValue(ns_string!("30"));
    row.limit.setStringValue(ns_string!("5"));
    send(&row.limit);
    assert_eq!(saved().auto_appclose.rules[0].max_windows, Some(5));
    // An idle-only rule saves and restores without a window limit.
    row.limit.setStringValue(ns_string!(""));
    row.idle.setStringValue(ns_string!("120"));
    send(&row.idle);
    assert_eq!(saved().auto_appclose.rules[0].max_windows, None);
    assert_eq!(saved().auto_appclose.rules[0].max_idle_minutes, Some(120));
    let idle_only = saved();
    row.idle.setStringValue(ns_string!("10081"));
    send(&row.idle);
    assert_eq!(saved(), idle_only, "an out-of-range idle time is not saved");
    row.idle.setStringValue(ns_string!("120"));
    send(&row.idle);
    page.enabled.setState(NSControlStateValueOn);
    send(&page.enabled);
    assert!(saved().auto_appclose.enabled);
    assert_eq!(saved().auto_appclose.rules[0].max_idle_minutes, Some(120));
    page.enabled.setState(NSControlStateValueOff);
    send(&page.enabled);
    assert!(!saved().auto_appclose.enabled);
    // Both limits can be set on one row.
    row.limit.setStringValue(ns_string!("5"));
    send(&row.limit);
    assert_eq!(saved().auto_appclose.rules[0].max_windows, Some(5));
    assert_eq!(saved().auto_appclose.rules[0].max_idle_minutes, Some(120));
    assert_eq!(saved().auto_appclose.rules[0].max_windows, Some(5));
    send(&row.remove);
    assert!(saved().auto_appclose.rules.is_empty());
    page.interval.setStringValue(ns_string!("10"));
    send(&page.interval);
}
