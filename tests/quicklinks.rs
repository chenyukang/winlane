use std::collections::HashMap;
use winlane::features::quicklinks::{self, Quicklink, Template};

fn link(name: &str, address: &str) -> Quicklink {
    Quicklink {
        id: name.into(),
        name: name.into(),
        link: address.into(),
        open_with: String::new(),
        shortcut: None,
    }
}
fn render(input: &str, clipboard: &str, values: &[(&str, &str)]) -> Result<String, String> {
    Template::parse(input)?.render(
        clipboard,
        &values
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        |_, _| "2026-09-18".into(),
    )
}
#[test]
fn shorthand_and_named_arguments_encode_values_without_changing_url_structure() {
    assert_eq!(
        render(
            "https://example.com/search?q={Query}&lang=zh",
            "",
            &[("Query", "Rust & 中文/#")]
        )
        .unwrap(),
        "https://example.com/search?q=Rust%20%26%20%E4%B8%AD%E6%96%87%2F%23&lang=zh"
    );
    let value = "https://example.com/{argument name=\"repo\"}/{argument name=\"repo\"}";
    assert_eq!(Template::parse(value).unwrap().arguments.len(), 1);
    assert_eq!(
        render(value, "", &[("repo", "a/b")]).unwrap(),
        "https://example.com/a%2Fb/a%2Fb"
    );
    assert!(render(value, "", &[]).is_err());
    assert_eq!(
        render(
            "https://example.com/?q={argument default=\"hello world\"}",
            "",
            &[]
        )
        .unwrap(),
        "https://example.com/?q=hello%20world"
    );
}
#[test]
fn clipboard_raw_dates_and_paths_expand_at_execution() {
    assert_eq!(
        render("https://example.com/?q={clipboard}", "a+b%20", &[]).unwrap(),
        "https://example.com/?q=a%2Bb%2520"
    );
    assert_eq!(
        render("{clipboard | raw}", "https://example.com/page?q=1", &[]).unwrap(),
        "https://example.com/page?q=1"
    );
    assert_eq!(
        render("https://example.com/{date format=\"yyyy-MM-dd\"}", "", &[]).unwrap(),
        "https://example.com/2026-09-18"
    );
    assert_eq!(
        render("~/Projects/{Query}", "", &[("Query", "Hello 世界")]).unwrap(),
        "~/Projects/Hello 世界"
    );
    assert_eq!(
        render("example.com/page", "", &[]).unwrap(),
        "https://example.com/page"
    );
    assert_eq!(
        render("example://open?q={Query}", "", &[("Query", "two words")]).unwrap(),
        "example://open?q=two%20words"
    );
}
#[test]
fn import_is_validated_atomic_and_repeatable() {
    let existing = vec![link("Existing", "https://example.test/")];
    let json = r#"[{"name":"Search","link":"https://example.com?q={Query}","iconName":"link","openWith":"Safari"},{"name":"Folder","link":"~/Downloads","openWith":"Finder"}]"#;
    let imported = quicklinks::import_json(&existing, json).unwrap();
    assert_eq!((imported.added, imported.skipped), (2, 0));
    assert_eq!(imported.links[1].open_with, "Safari");
    assert_eq!(imported.links[0], existing[0]);
    let repeated = quicklinks::import_json(&imported.links, json).unwrap();
    assert_eq!((repeated.added, repeated.skipped), (0, 2));
    assert_eq!(repeated.links, imported.links);
    assert!(
        quicklinks::import_json(
            &existing,
            r#"[{"name":"Good","link":"https://example.com"},{"name":"Bad","link":"invalid"}]"#
        )
        .is_err()
    );
    assert_eq!(existing.len(), 1);
    for json in ["{}", "null", r#"[{"name":1,"link":"https://example.com"}]"#] {
        assert!(quicklinks::import_json(&existing, json).is_err());
    }
}
#[test]
fn bounds_and_invalid_templates_are_rejected() {
    for address in [
        "",
        "invalid",
        "https://",
        "https:///x",
        "javascript:alert(1)",
        "https://example.com/{argument name=}",
        "https://example.com/{Query",
        "https://example.com/{clipboard | unknown}",
        "https://example.com/\n",
    ] {
        assert!(
            link("Invalid", address).validate().is_err(),
            "unexpected accepted input"
        );
    }
    assert!(
        link(
            "Too long",
            &format!(
                "https://example.com/{}",
                "x".repeat(quicklinks::MAX_LINK_BYTES)
            )
        )
        .validate()
        .is_err()
    );
    assert!(render("https://example.com/{clipboard}", &"中".repeat(10_000), &[]).is_err());
    let a = link("One", "https://example.com/");
    assert!(quicklinks::validate(&[a.clone(), a]).is_err());
}
#[test]
fn search_and_config_roundtrip_preserve_links() {
    let links = vec![
        link("Rust docs", "https://example.com/rust"),
        link("Search", "https://example.com/{Query}"),
        link("Search issues", "https://example.test/issues"),
    ];
    assert_eq!(quicklinks::matching(&links, " search "), vec![1, 2]);
    assert_eq!(quicklinks::matching(&links, "RUST docs"), vec![0]);
    assert_eq!(quicklinks::matching(&links, ""), vec![0, 1, 2]);
    let old = winlane::core::config::Config::from_json("{}").unwrap();
    assert!(old.quicklinks.is_empty());
    let config = winlane::core::config::Config {
        quicklinks: links,
        ..old
    };
    assert_eq!(
        winlane::core::config::Config::from_json(&config.to_json().unwrap()).unwrap(),
        config
    );
}
#[test]
fn distinct_unnamed_arguments_and_repeated_field_conflicts() {
    let template = Template::parse("https://example.com/{argument}/{argument}").unwrap();
    assert_eq!(template.arguments.len(), 2);
    assert_ne!(template.arguments[0].name, template.arguments[1].name);
    let values: HashMap<_, _> = template
        .arguments
        .iter()
        .enumerate()
        .map(|(i, a)| (a.name.clone(), i.to_string()))
        .collect();
    assert_eq!(
        template.render("", &values, |_, _| String::new()).unwrap(),
        "https://example.com/0/1"
    );
    assert!(
        Template::parse("https://example.com/{Query}/{argument name=\"Query\" default=\"other\"}")
            .is_err()
    );
}
