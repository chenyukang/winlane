use winlane::core::{
    config::{ApplicationTarget, Config},
    input_method::InputMethod,
};
use winlane::features::input_rules::*;

fn resolve(rule: &SourceRule) -> Option<String> {
    match rule {
        SourceRule::English => Some("en".into()),
        SourceRule::Chinese => Some("zh".into()),
        SourceRule::Source(source) => (source.id != "missing").then(|| source.id.clone()),
        _ => None,
    }
}
fn app(id: &str, source: SourceRule, restore: Option<RestoreStrategy>) -> AppRule {
    AppRule {
        application: ApplicationTarget {
            bundle_id: id.into(),
            name: "Example".into(),
            path: "/Applications/Example.app".into(),
        },
        source,
        restore,
    }
}
fn activate(
    runtime: &mut Runtime,
    settings: &Settings,
    app: &str,
    current: &str,
) -> Option<String> {
    runtime.activate(settings, Some((app, 1)), Some(current), resolve)
}
#[test]
fn legacy_settings_migrate_without_retaining_a_second_input_policy() {
    for policy in [
        InputMethod::Current,
        InputMethod::English,
        InputMethod::Chinese,
        InputMethod::LastUsed,
    ] {
        let json = format!(
            r#"{{"input_method":{}}}"#,
            serde_json::to_string(&policy).unwrap()
        );
        let config = Config::from_json(&json).unwrap();
        assert_eq!(config.input_rules.winlane_policy(), policy);
        assert!(!config.input_rules.enabled);
        assert!(!config.to_json().unwrap().contains("input_method"));
        assert_eq!(
            Config::from_json(&config.to_json().unwrap()).unwrap(),
            config
        );
    }
    let config = Config::from_json("{}").unwrap();
    assert_eq!(config.input_rules.winlane_policy(), InputMethod::English);
    assert_eq!(config.input_rules.apps[0].application.bundle_id, WINLANE_ID);
}
#[test]
fn app_overrides_global_and_duplicate_activation_does_not_force_manual_changes() {
    let mut runtime = Runtime::default();
    let mut settings = Settings {
        enabled: true,
        default_source: SourceRule::English,
        ..Settings::default()
    };
    settings
        .apps
        .push(app("com.example.editor", SourceRule::Chinese, None));
    assert_eq!(
        activate(&mut runtime, &settings, "com.example.browser", "zh"),
        Some("en".into())
    );
    assert_eq!(
        activate(&mut runtime, &settings, "com.example.editor", "en"),
        Some("zh".into())
    );
    runtime.observe(&settings, "com.example.editor", 1, "en");
    assert_eq!(
        activate(&mut runtime, &settings, "com.example.editor", "en"),
        None
    );
    assert_eq!(activate(&mut runtime, &settings, WINLANE_ID, "en"), None);
    assert_eq!(
        activate(&mut runtime, &settings, "com.example.editor", "en"),
        Some("zh".into())
    );
    settings.enabled = false;
    assert_eq!(
        activate(&mut runtime, &settings, "com.example.browser", "zh"),
        None
    );
}
#[test]
fn last_used_is_per_app_and_unavailable_history_falls_back_to_default() {
    let mut runtime = Runtime::default();
    let mut settings = Settings {
        enabled: true,
        default_source: SourceRule::English,
        restore: RestoreStrategy::LastUsed,
        ..Settings::default()
    };
    assert_eq!(
        activate(&mut runtime, &settings, "com.example.one", "zh"),
        Some("en".into())
    );
    runtime.observe(&settings, "com.example.one", 1, "zh");
    // Late notifications from another app or an old PID cannot contaminate history.
    runtime.observe(&settings, "com.example.two", 1, "wrong");
    runtime.observe(&settings, "com.example.one", 2, "wrong");
    activate(&mut runtime, &settings, "com.example.two", "en");
    runtime.observe(&settings, "com.example.two", 1, "ja");
    assert_eq!(
        activate(&mut runtime, &settings, "com.example.one", "ja"),
        Some("zh".into())
    );
    let history = runtime.history();
    let mut restored = Runtime::default();
    restored.restore_history(&history);
    assert_eq!(
        activate(&mut restored, &settings, "com.example.two", "zh"),
        Some("ja".into())
    );
    restored.observe(&settings, "com.example.two", 1, "missing");
    activate(&mut restored, &settings, "com.example.one", "zh");
    assert_eq!(
        activate(&mut restored, &settings, "com.example.two", "zh"),
        Some("en".into())
    );
    settings.apps.push(app(
        "com.example.two",
        SourceRule::Current,
        Some(RestoreStrategy::Default),
    ));
    restored.reset_activation();
    assert_eq!(
        activate(&mut restored, &settings, "com.example.two", "zh"),
        None
    );
}
#[test]
fn validation_and_unavailable_sources_preserve_configuration() {
    let mut settings = Settings::default();
    settings.apps.push(app(
        "com.example.one",
        SourceRule::Source(InputSource {
            id: "missing".into(),
            name: "Unavailable".into(),
        }),
        None,
    ));
    settings.validate().unwrap();
    let mut runtime = Runtime::default();
    settings.enabled = true;
    assert_eq!(
        activate(&mut runtime, &settings, "com.example.one", "zh"),
        None
    );
    settings.apps.push(settings.apps[1].clone());
    assert!(settings.validate().is_err());
    settings.apps.pop();
    settings.apps.remove(0);
    assert!(settings.validate().is_err());
    runtime.restore_history("invalid");
    assert!(!runtime.history().contains("invalid"));
}
