use winlane::search::{WindowInfo, rank};

fn window(id: u64, app: &str, title: &str) -> WindowInfo {
    WindowInfo {
        id,
        pid: id as i32,
        app: app.into(),
        title: title.into(),
        minimized: false,
    }
}

#[test]
fn title_prefix_and_contiguous_matches_outrank_scattered_characters() {
    let windows = [
        window(1, "Editor", "Reusing testers"),
        window(2, "Browser", "Learning Rust"),
        window(3, "Terminal", "Rust compiler"),
    ];
    assert_eq!(rank(&windows, "rust", None), vec![2, 1, 0]);
}

#[test]
fn tokens_can_match_app_and_title_in_either_order() {
    let windows = [
        window(1, "Safari", "Rust compiler guide"),
        window(2, "Firefox", "Rust compiler guide"),
        window(3, "Safari", "Grocery list"),
    ];
    assert_eq!(rank(&windows, "saf rust", None), vec![0]);
    assert_eq!(rank(&windows, "rust saf", None), vec![0]);
}

#[test]
fn acronym_matches_outrank_scattered_matches() {
    let windows = [
        window(1, "Vivid scenery", "Untitled"),
        window(2, "Visual Studio Code", "Untitled"),
    ];
    assert_eq!(rank(&windows, "vsc", None), vec![1, 0]);
    assert_eq!(rank(&[window(3, "FileBrowser", "")], "fb", None), vec![0]);
}

#[test]
fn all_tokens_must_match_and_characters_keep_their_order() {
    let windows = [window(1, "Editor", "Rust"), window(2, "Browser", "Notes")];
    assert!(rank(&windows, "rust missing", None).is_empty());
    assert!(rank(&windows, "tsur", None).is_empty());
    assert!(rank(&windows, "zzz", None).is_empty());
    assert!(rank(&[], "rust", None).is_empty());
}

#[test]
fn unicode_case_and_chinese_titles_match() {
    let windows = [
        window(1, "ÉDITEUR", "Résumé"),
        window(2, "浏览器", "理解 Rust 编译器"),
    ];
    assert_eq!(rank(&windows, "éditeur RÉSUMÉ", None), vec![0]);
    assert_eq!(rank(&windows, "浏览 编译", None), vec![1]);
    assert_eq!(rank(&windows, "理解 rust", None), vec![1]);
}

#[test]
fn unicode_whitespace_separates_tokens() {
    let windows = [window(1, "Safari", "Rust compiler")];
    assert_eq!(rank(&windows, " \tSAF\n rust\u{3000}", None), vec![0]);
}

#[test]
fn preference_boosts_matching_windows_without_admitting_nonmatches() {
    let windows = [
        window(1, "Editor", "Rust"),
        window(2, "Editor", "Rust"),
        window(3, "Browser", "Notes"),
    ];
    assert_eq!(rank(&windows, "rust", None), vec![0, 1]);
    assert_eq!(rank(&windows, "rust", Some(2)), vec![1, 0]);
    assert_eq!(rank(&windows, "rust", Some(3)), vec![0, 1]);
}

#[test]
fn empty_query_keeps_original_order_even_with_a_preference() {
    let windows = [
        window(7, "Zebra", "Last"),
        window(3, "Apple", "First"),
        window(5, "Middle", "Between"),
    ];
    assert_eq!(rank(&windows, "", Some(5)), vec![0, 1, 2]);
    assert_eq!(rank(&windows, " \n\u{3000}\t", Some(3)), vec![0, 1, 2]);
}

#[test]
fn oversized_query_does_not_match_a_truncated_prefix() {
    let long = "x".repeat(200);
    assert!(rank(&[window(1, &long, "")], &long, None).is_empty());
}

#[test]
fn spelling_errors_match_application_names_and_project_words() {
    let chrome = [window(1, "Google Chrome", "Documentation")];
    for query in ["chorme", "chroe", "chroome", "chrxme", " CHORME "] {
        assert_eq!(rank(&chrome, query, None), [0], "query {query}");
    }
    for title in [
        "channel_signer.rs (Working Tree) — fiber",
        "project/fiber/src/channel_signer.rs",
        "MyFiberWorkspace",
        "École — workspace",
    ] {
        let query = if title.starts_with('É') {
            "écloe"
        } else {
            "fibre"
        };
        let windows = [window(1, "Code", title)];
        assert_eq!(rank(&windows, query, None), [0], "title {title}");
        assert_eq!(rank(&windows, &format!("code {query}"), None), [0]);
        assert!(rank(&windows, &format!("{query} missing"), None).is_empty());
    }
}

#[test]
fn typo_tolerance_is_bounded_and_does_not_expand_short_queries() {
    let chrome = [window(1, "Chrome", "")];
    assert!(rank(&chrome, "chxxme", None).is_empty());
    let obsidian = [window(1, "Obsidian", "")];
    assert_eq!(rank(&obsidian, "obsidxxn", None), [0]);
    assert!(rank(&obsidian, "obxxdxxn", None).is_empty());
    for (query, target) in [("z", "a"), ("co", "go"), ("fir", "far")] {
        assert!(rank(&[window(1, target, "")], query, None).is_empty());
    }
    assert_eq!(rank(&[window(1, "Code", "fiber")], "fi", None), [0]);
}

#[test]
fn fewer_edits_outrank_more_edits_even_with_a_saved_preference() {
    let windows = [window(1, "Obsidian", ""), window(2, "Obsidixn", "")];
    assert_eq!(rank(&windows, "obsidxxn", Some(1)), [1, 0]);
}
