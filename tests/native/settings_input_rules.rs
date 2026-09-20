use super::*;

pub(crate) fn verify(target: &AnyObject, mtm: MainThreadMarker) {
    let settings = SettingsWindow::new(target, mtm);
    let mut config = Config::default();
    config.input_rules.apps.push(AppRule {
        application: ApplicationTarget {
            bundle_id: "com.example.editor".into(),
            name: "Example Editor".into(),
            path: "/Applications/Example.app".into(),
        },
        source: SourceRule::Source(InputSource {
            id: "example.unavailable".into(),
            name: "Unavailable".into(),
        }),
        restore: Some(RestoreStrategy::LastUsed),
    });
    settings.fill(&config);
    assert_eq!(
        settings.candidate().unwrap(),
        config,
        "lazy page preserves rules"
    );
    settings.select_tab(9);
    let page = settings.input_rules();
    assert_eq!(settings.candidate().unwrap(), config);
    for control in [
        &*page.enabled as &NSControl,
        &*page.source.control,
        &*page.restore,
    ] {
        assert_eq!(control.action(), Some(sel!(settingsChanged:)));
    }
    let rows = page.rows.borrow().clone();
    assert!(!rows[0].remove.isEnabled());
    assert!(!rows[0].choose.isEnabled());
    assert!(rows[1].remove.isEnabled());
    assert_eq!(rows[1].source.read(), config.input_rules.apps[1].source);
    page.refresh_sources();
    assert_eq!(
        settings.candidate().unwrap(),
        config,
        "missing source stays selected"
    );
    page.enabled.setState(NSControlStateValueOn);
    page.source.fill(SourceRule::Chinese, &page.sources());
    page.restore.selectItemAtIndex(1);
    rows[1].source.control.selectItemAtIndex(0);
    rows[1].restore.selectItemAtIndex(0);
    let updated = settings.candidate().unwrap();
    assert_eq!(
        updated.input_rules.resolve("com.example.editor"),
        (&SourceRule::Chinese, RestoreStrategy::LastUsed)
    );
    assert_eq!(
        updated.input_rules.winlane_policy(),
        InputMethod::English,
        "Winlane default still overrides global"
    );
    rows[0].source.control.selectItemAtIndex(0);
    rows[0].restore.selectItemAtIndex(0);
    assert_eq!(
        settings
            .candidate()
            .unwrap()
            .input_rules
            .resolve(WINLANE_ID),
        (&SourceRule::Chinese, RestoreStrategy::LastUsed)
    );
    settings.remove_input_rule(0);
    assert_eq!(page.rows.borrow().len(), 2, "built-in rule stays available");
    settings.remove_input_rule(1);
    assert_eq!(settings.candidate().unwrap().input_rules.apps.len(), 1);
    let invalid = page.append_row();
    InputRulesPage::set_application(&invalid, config.input_rules.apps[0].application.clone());
    assert!(
        settings.candidate().is_err(),
        "duplicate app rule must not save"
    );
    assert!(!settings.window.isVisible());
    settings.window.close();
    println!(
        "Input rules settings: global/app inheritance, protected Winlane default, missing sources, validation and lazy settings verified with hidden windows."
    );
}

pub(crate) fn verify_autosave(settings: &SettingsWindow, saved: impl Fn() -> Config) {
    let page = settings.input_rules();
    let row = page.rows.borrow()[0].clone();
    row.source.control.selectItemAtIndex(3);
    // SAFETY: Exercise the actual native target/action without selecting a system input source.
    unsafe {
        row.source.control.sendAction_to(
            row.source.control.action(),
            row.source.control.target().as_deref(),
        );
    }
    assert_eq!(saved().input_rules.winlane_policy(), InputMethod::Chinese);
    page.enabled.setState(NSControlStateValueOn);
    unsafe {
        page.enabled
            .sendAction_to(page.enabled.action(), page.enabled.target().as_deref());
    }
    assert!(saved().input_rules.enabled);
}
