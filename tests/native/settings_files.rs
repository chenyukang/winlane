use super::*;
use objc2_foundation::ns_string;

pub(crate) fn verify(target: &AnyObject, mtm: MainThreadMarker) {
    let settings = SettingsWindow::new(target, mtm);
    let mut config = Config::default();
    config.files.roots = vec!["~/Documents".into(), "/Volumes/Archive".into()];
    settings.fill(&config);
    assert!(settings.files.get().is_none());
    assert_eq!(settings.candidate().unwrap(), config);
    settings.select_tab(11);
    let page = settings.files();
    assert_eq!(settings.candidate().unwrap(), config);
    assert_eq!(
        page.roots.stringValue().to_string(),
        "~/Documents; /Volumes/Archive"
    );
    for field in [&page.roots, &page.excluded] {
        assert_eq!(field.action(), Some(sel!(settingsChanged:)));
        assert!(field.cell().unwrap().sendsActionOnEndEditing());
    }
    page.roots.setStringValue(ns_string!("relative"));
    assert!(settings.candidate().is_err());
    page.roots
        .setStringValue(ns_string!("~/Documents; /Volumes/External Disk"));
    page.excluded
        .setStringValue(ns_string!("~/Documents/Private"));
    page.generated.setState(NSControlStateValueOff);
    let candidate = settings.candidate().unwrap();
    assert_eq!(
        candidate.files.roots,
        ["~/Documents", "/Volumes/External Disk"]
    );
    assert_eq!(candidate.files.excluded, ["~/Documents/Private"]);
    assert!(!candidate.files.hide_generated);
    assert!(!settings.window.isVisible());
    settings.window.close();
}

pub(crate) fn verify_autosave(settings: &SettingsWindow, saved: impl Fn() -> Config) {
    let page = settings.files();
    let send = |control: &NSControl| unsafe {
        assert!(control.sendAction_to(control.action(), control.target().as_deref()));
    };
    page.roots
        .setStringValue(ns_string!("~/Documents; ~/Downloads"));
    send(&page.roots);
    assert_eq!(saved().files.roots, ["~/Documents", "~/Downloads"]);
    let valid = saved();
    page.roots.setStringValue(ns_string!(""));
    send(&page.roots);
    assert_eq!(saved(), valid);
    page.roots
        .setStringValue(ns_string!("~/Documents; ~/Downloads"));
}
