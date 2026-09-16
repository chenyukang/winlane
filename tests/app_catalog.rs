use std::collections::HashSet;
use winlane::app_catalog::{InstalledApp, matching_apps};
use winlane::config::ApplicationTarget;

fn app(id: &str, name: &str, other_names: &[&str]) -> InstalledApp {
    InstalledApp {
        target: ApplicationTarget {
            bundle_id: id.into(),
            name: name.into(),
            path: format!("/Applications/{name}.app"),
        },
        names: other_names.iter().map(|name| (*name).into()).collect(),
    }
}

#[test]
fn launch_results_require_a_nonempty_bounded_query() {
    let apps = [app("example.a", "Alpha", &[])];
    for query in ["", " \n\u{3000}", &"a".repeat(129)] {
        assert!(matching_apps(&apps, query, &HashSet::new(), &[], None).is_empty());
    }
    assert_eq!(
        matching_apps(&apps, "alpha", &HashSet::new(), &[], None),
        [0]
    );
}

#[test]
fn localized_names_english_names_acronyms_and_keywords_match() {
    let apps = [
        app("example.wechat", "微信", &["WeChat"]),
        app("example.code", "Code", &["Visual Studio Code"]),
    ];
    for query in ["微信", "wechat", "WECH", "we ch"] {
        assert_eq!(matching_apps(&apps, query, &HashSet::new(), &[], None), [0]);
    }
    assert_eq!(matching_apps(&apps, "vsc", &HashSet::new(), &[], None), [1]);
    assert!(matching_apps(&apps, "code missing", &HashSet::new(), &[], None).is_empty());
    assert!(matching_apps(&apps, "applications", &HashSet::new(), &[], None).is_empty());
}

#[test]
fn existing_windows_and_exclusions_remove_launch_duplicates() {
    let apps = [
        app("example.browser", "Browser", &[]),
        app("example.chat", "Browser Chat", &["聊天"]),
    ];
    let opened = HashSet::from(["example.browser".into()]);
    assert_eq!(matching_apps(&apps, "browser", &opened, &[], None), [1]);
    assert!(matching_apps(&apps, "browser", &opened, &["browser chat".into()], None).is_empty());
    assert!(matching_apps(&apps, "browser", &opened, &["聊天".into()], None).is_empty());
    assert_eq!(
        matching_apps(&apps, "browser", &HashSet::new(), &[], None),
        [0, 1]
    );
}

#[test]
fn saved_app_aliases_can_launch_but_do_not_duplicate_windows() {
    let apps = [
        app("example.wechat", "WeChat", &[]),
        app("example.warp", "Warp", &[]),
    ];
    assert_eq!(
        matching_apps(&apps, "w", &HashSet::new(), &[], Some("example.wechat")),
        [0]
    );
    assert!(
        matching_apps(
            &apps,
            "w",
            &HashSet::from(["example.wechat".into()]),
            &[],
            Some("example.wechat")
        )
        .is_empty()
    );
}
