use winlane::core::config::{AppShortcut, ApplicationTarget, Config, Shortcut, SortOrder};
use winlane::core::i18n::{Language, Locale};

#[test]
fn system_language_uses_the_first_supported_preference() {
    for tag in ["zh", "zh-Hans-CN", "zh-Hant-TW", "zh_CN", "ZH-hk"] {
        assert_eq!(Language::System.resolve([tag, "en-US"]), Locale::Chinese);
    }
    for tag in ["en", "en-US", "en-GB", "en_AU"] {
        assert_eq!(Language::System.resolve([tag, "zh-Hans"]), Locale::English);
    }
    assert_eq!(
        Language::System.resolve(["ja-JP", "zh-Hans"]),
        Locale::Chinese
    );
    assert_eq!(Language::System.resolve(["fr", "de"]), Locale::English);
    assert_eq!(Language::System.resolve([]), Locale::English);
}

#[test]
fn explicit_choice_overrides_system_preferences() {
    assert_eq!(Language::Chinese.resolve(["en-US"]), Locale::Chinese);
    assert_eq!(Language::English.resolve(["zh-Hans"]), Locale::English);
}

#[test]
fn language_migration_and_round_trip_preserve_existing_preferences() {
    let legacy = Config::from_json(r#"{"sort":"Title","excluded_apps":["Finder"]}"#).unwrap();
    assert_eq!(legacy.language, Language::System);
    assert_eq!(legacy.sort, SortOrder::Title);
    for language in [Language::System, Language::Chinese, Language::English] {
        let config = Config {
            language,
            app_shortcuts: vec![AppShortcut {
                shortcut: Shortcut {
                    command: true,
                    control: false,
                    option: false,
                    shift: false,
                    key: "Digit3".into(),
                },
                application: ApplicationTarget {
                    bundle_id: "com.example.chat".into(),
                    path: "/Applications/Example Chat.app".into(),
                    name: "Example Chat".into(),
                },
            }],
            ..legacy.clone()
        };
        assert_eq!(
            Config::from_json(&config.to_json().unwrap()).unwrap(),
            config
        );
    }
    assert!(Config::from_json(r#"{"language":"Invalid"}"#).is_err());
}

#[test]
fn text_and_formatted_messages_change_together() {
    for locale in [Locale::English, Locale::Chinese, Locale::English] {
        winlane::core::i18n::set_locale(locale);
        let name = "Example";
        let text = winlane::tr!("设置", "Settings");
        let message = winlane::trf!("启动 {name} · {}", "Launch {name} · {}", 3);
        if locale == Locale::Chinese {
            assert_eq!(text, "设置");
            assert_eq!(message, "启动 Example · 3");
        } else {
            assert_eq!(text, "Settings");
            assert_eq!(message, "Launch Example · 3");
        }
    }
}
