use winlane::features::window_labels::{
    MAX_LABEL_CHARS, WindowIdentity, WindowLabel, label_for, normalize_title, set_label, validate,
};

fn window<'a>(title: &'a str, server_id: Option<u32>) -> WindowIdentity<'a> {
    WindowIdentity {
        bundle_id: "com.microsoft.VSCode",
        title,
        server_id,
    }
}

#[test]
fn titles_compare_ignoring_spacing_and_case() {
    assert_eq!(
        normalize_title("  Model   1  ·  Train  "),
        "model 1 · train"
    );
    assert_eq!(normalize_title("MODEL 1 · TRAIN"), "model 1 · train");
    assert!(normalize_title("").is_empty());
}

#[test]
fn a_label_replaces_the_title_until_it_is_cleared() {
    let mut labels = Vec::new();
    let target = window("train log — project", Some(7));
    set_label(&mut labels, &target, "  模型 1 · 训练日志 ").unwrap();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].label, "模型 1 · 训练日志");
    assert_eq!(
        label_for(&labels, &target).map(|l| l.label.as_str()),
        Some("模型 1 · 训练日志")
    );
    // The same window with the same title keeps its label even when the
    // WindowServer assigns a different identity after a restart.
    let renamed = window("train log — project", Some(99));
    assert_eq!(
        label_for(&labels, &renamed).map(|l| l.label.as_str()),
        Some("模型 1 · 训练日志")
    );
    // An empty label removes it.
    set_label(&mut labels, &renamed, "   ").unwrap();
    assert!(labels.is_empty());
}

#[test]
fn two_windows_with_the_same_title_keep_their_own_label() {
    let mut labels = Vec::new();
    set_label(&mut labels, &window("index.ts", Some(1)), "左侧编辑器").unwrap();
    set_label(&mut labels, &window("index.ts", Some(2)), "右侧编辑器").unwrap();
    assert_eq!(labels.len(), 2, "each window keeps its own label");
    assert_eq!(
        label_for(&labels, &window("index.ts", Some(1))).map(|l| l.label.as_str()),
        Some("左侧编辑器")
    );
    assert_eq!(
        label_for(&labels, &window("index.ts", Some(2))).map(|l| l.label.as_str()),
        Some("右侧编辑器")
    );
    // A window without an identity still matches by app and title.
    assert!(label_for(&labels, &window("index.ts", None)).is_some());
}

#[test]
fn a_changed_title_keeps_the_name_on_its_window() {
    let mut labels = Vec::new();
    set_label(&mut labels, &window("index.ts", Some(5)), "左侧编辑器").unwrap();
    // The editor appends a suffix while the same window stays open.
    let renamed = window("index.ts — project (modified)", Some(5));
    assert_eq!(
        label_for(&labels, &renamed).map(|label| label.label.as_str()),
        Some("左侧编辑器"),
        "the WindowServer identity outranks a changed title"
    );
}

#[test]
fn renaming_a_window_replaces_rather_than_duplicates() {
    let mut labels = Vec::new();
    let target = window("index.ts", Some(3));
    set_label(&mut labels, &target, "旧的").unwrap();
    set_label(&mut labels, &target, "新的").unwrap();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].label, "新的");
}

#[test]
fn labels_are_bounded_and_checked_when_loaded() {
    let mut labels = Vec::new();
    let too_long = "x".repeat(MAX_LABEL_CHARS + 1);
    assert!(set_label(&mut labels, &window("a", Some(1)), &too_long).is_err());
    assert!(labels.is_empty(), "a rejected name is not stored");

    labels.push(WindowLabel {
        bundle_id: "app".into(),
        title_key: "t".into(),
        server_id: None,
        label: "n".into(),
    });
    validate(&labels).unwrap();

    let mut duplicate = labels.clone();
    duplicate.push(labels[0].clone());
    assert!(validate(&duplicate).is_err(), "duplicates are rejected");
    let mut empty_app = labels.clone();
    empty_app[0].bundle_id.clear();
    assert!(validate(&empty_app).is_err(), "an empty app is rejected");
    let mut blank = labels.clone();
    blank[0].label.clear();
    assert!(validate(&blank).is_err(), "an empty name is rejected");
    let mut overlong = labels.clone();
    overlong[0].label = too_long;
    assert!(validate(&overlong).is_err(), "an overlong name is rejected");
}
