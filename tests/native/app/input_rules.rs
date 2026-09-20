use super::*;
use winlane::features::input_rules::{InputSource, RestoreStrategy, Settings, SourceRule};

pub(super) fn verify(mtm: MainThreadMarker) {
    let before = Source::current(mtm).and_then(|source| source.id());
    let delegate = responsive_fixture(mtm);
    for source in Source::enabled(mtm) {
        let rule = SourceRule::Source(InputSource {
            id: source.id.clone(),
            name: source.name,
        });
        let mut rules = Settings {
            enabled: true,
            default_source: SourceRule::Chinese,
            ..Settings::default()
        };
        rules.apps[0].source = rule;
        rules.apps[0].restore = Some(RestoreStrategy::Default);
        delegate.ivars().config.borrow_mut().input_rules = rules.clone();
        delegate.prepare_panel(PanelMode::Search, 1, 0);
        assert_eq!(
            delegate
                .ivars()
                .input_target
                .borrow()
                .as_ref()
                .and_then(Source::id),
            Some(source.id.clone())
        );
        assert_eq!(
            delegate.ivars().input_gate.borrow().target(),
            Some(source.id.as_str())
        );
        let layout = Source::by_id(&source.id, mtm).unwrap().is_keyboard_layout();
        assert_eq!(
            delegate.ivars().input_layout_source.borrow().as_deref(),
            layout.then_some(source.id.as_str())
        );
        assert!(!delegate.any_panel_visible());
        delegate.request_app_input_rules();
        let history = delegate.ivars().app_input_rules.borrow().history();
        delegate.remember_app_input();
        assert_eq!(delegate.ivars().app_input_rules.borrow().history(), history);
        assert!(
            delegate.ivars().app_input_pending.get(),
            "external rules wait while Winlane owns input"
        );
        delegate.end_session();
    }
    // Keeping current disables buffering; an unavailable concrete source is not replaced
    // by an unrelated global default, and does not break opening the search field.
    for source in [
        SourceRule::Current,
        SourceRule::Source(InputSource {
            id: "example.missing".into(),
            name: "Unavailable".into(),
        }),
    ] {
        delegate.ivars().config.borrow_mut().input_rules.apps[0].source = source;
        delegate.prepare_panel(PanelMode::Search, 2, 0);
        assert!(delegate.ivars().input_target.borrow().is_none());
        assert!(delegate.ivars().input_gate.borrow().target().is_none());
        delegate.end_session();
    }
    for panel in delegate.panels() {
        panel.panel.close();
    }
    assert_eq!(Source::current(mtm).and_then(|source| source.id()), before);
    println!(
        "Unified Winlane input rules: concrete sources prepare before focus, keyboard-layout bypass and startup buffering are retained, external rules cannot overwrite active Winlane input; no system input source selected."
    );
}
