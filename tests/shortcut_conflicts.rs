use winlane::core::commands::CommandId;
use winlane::core::config::{AppShortcut, ApplicationTarget, CommandShortcut, Config, Shortcut};
use winlane::core::i18n::{self, Locale};
use winlane::features::quicklinks::Quicklink;

fn shortcut(key: &str) -> Shortcut {
    Shortcut {
        option: true,
        key: key.into(),
        ..Shortcut::default()
    }
}

fn fixture() -> Config {
    Config {
        additional_search_shortcuts: vec![shortcut("KeyS")],
        switch_shortcut: Shortcut {
            key: "Tab".into(),
            ..Shortcut::default()
        },
        app_shortcuts: [("Browser", "KeyB"), ("Editor", "KeyE")]
            .into_iter()
            .map(|(name, key)| AppShortcut {
                shortcut: shortcut(key),
                application: ApplicationTarget {
                    name: name.into(),
                    bundle_id: format!("com.example.{name}"),
                    path: format!("/Applications/{name}.app"),
                },
            })
            .collect(),
        quicklinks: [("google-search", "KeyG"), ("Docs", "KeyD")]
            .into_iter()
            .map(|(name, key)| Quicklink {
                id: name.into(),
                name: name.into(),
                link: "https://example.com".into(),
                open_with: String::new(),
                shortcut: Some(shortcut(key)),
            })
            .collect(),
        command_shortcuts: [(CommandId::OpenUrl, "KeyU"), (CommandId::Projects, "KeyP")]
            .into_iter()
            .map(|(command, key)| CommandShortcut {
                command,
                shortcut: shortcut(key),
            })
            .collect(),
        ..Config::default()
    }
}

fn binding(config: &mut Config, index: usize) -> &mut Shortcut {
    match index {
        0 => &mut config.shortcut,
        1 => &mut config.additional_search_shortcuts[0],
        2 => &mut config.switch_shortcut,
        3 | 4 => &mut config.app_shortcuts[index - 3].shortcut,
        5 | 6 => config.quicklinks[index - 5].shortcut.as_mut().unwrap(),
        7 | 8 => &mut config.command_shortcuts[index - 7].shortcut,
        _ => unreachable!(),
    }
}

#[test]
fn conflicts_identify_both_owners_and_the_key_combination_in_both_languages() {
    let previous = i18n::locale();
    for (locale, labels, reverse_label) in [
        (
            Locale::English,
            [
                "Search mode (shortcut 1)",
                "Search mode (shortcut 2)",
                "Switch mode",
                "App “Browser”",
                "App “Editor”",
                "Quicklink “google-search”",
                "Quicklink “Docs”",
                "Command “open-url”",
                "Command “projects”",
            ],
            "Switch mode (reverse)",
        ),
        (
            Locale::Chinese,
            [
                "搜索模式（快捷键 1）",
                "搜索模式（快捷键 2）",
                "切换模式",
                "应用“Browser”",
                "应用“Editor”",
                "快捷链接“google-search”",
                "快捷链接“Docs”",
                "命令“open-url”",
                "命令“projects”",
            ],
            "切换模式（反向）",
        ),
    ] {
        i18n::set_locale(locale);
        let original = fixture();
        original.validate().unwrap();
        for edited in 0..labels.len() {
            for occupied in 0..labels.len() {
                if edited == occupied {
                    continue;
                }
                let mut candidate = original.clone();
                let duplicate = binding(&mut candidate, occupied).clone();
                *binding(&mut candidate, edited) = duplicate.clone();
                let error = candidate.validate().unwrap_err();
                for expected in [labels[edited], labels[occupied], &duplicate.display()] {
                    assert!(
                        error.contains(expected),
                        "missing {expected:?} in {error:?}"
                    );
                }
            }
            if edited != 2 {
                let mut candidate = original.clone();
                let mut reverse = candidate.switch_shortcut.clone();
                reverse.shift = true;
                *binding(&mut candidate, edited) = reverse.clone();
                let error = candidate.validate().unwrap_err();
                for expected in [labels[edited], reverse_label, &reverse.display()] {
                    assert!(
                        error.contains(expected),
                        "missing {expected:?} in {error:?}"
                    );
                }
            }
        }
        let mut shifted_switch = original;
        shifted_switch.switch_shortcut.shift = true;
        shifted_switch.shortcut = Shortcut {
            key: "Tab".into(),
            ..Shortcut::default()
        };
        shifted_switch.validate().unwrap();
    }
    i18n::set_locale(previous);
}
