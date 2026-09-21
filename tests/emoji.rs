use std::collections::HashSet;
use winlane::features::emoji::{MAX_RESULTS, catalog, matching};

#[test]
fn bundled_catalog_is_complete_unique_and_bilingual() {
    let entries = catalog();
    assert_eq!(entries.len(), 3781);
    let unique: HashSet<_> = entries.iter().map(|entry| entry.text).collect();
    assert_eq!(entries.len(), unique.len());
    assert!(entries.iter().all(|entry| !entry.text.is_empty()
        && !entry.name_en.is_empty()
        && !entry.name_zh.is_empty()));
    assert_eq!(matching("rocket")[0].name_zh, "火箭");
}

#[test]
fn matches_both_languages_and_normalized_names() {
    for (query, expected) in [
        ("rocket", "🚀"),
        ("火箭", "🚀"),
        (" ROCKET ", "🚀"),
        (":thumbs_up:", "👍"),
        ("red-heart", "❤️"),
        ("红心", "❤️"),
    ] {
        assert_eq!(matching(query)[0].text, expected, "{query}");
    }
    for query in ["cat", "猫"] {
        assert!(matching(query).iter().any(|entry| entry.text == "🐈"));
    }
    assert!(matching("zzzzzz-no-such-emoji").is_empty());
    assert!(matching("::").is_empty());
}

#[test]
fn preserves_complete_emoji_sequences_and_skin_tones() {
    for text in ["❤️", "👨‍👩‍👧‍👦", "👩🏽‍💻", "🇨🇳", "1️⃣"] {
        let results = matching(text);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, text);
    }
    assert_eq!(matching("❤")[0].text, "❤️");
    assert!(
        matching("thumbs up medium skin tone")
            .iter()
            .any(|entry| entry.text == "👍🏽")
    );
    assert_eq!(matching("thumbs up")[0].text, "👍");
}

#[test]
fn results_are_bounded_with_useful_empty_choices() {
    let empty = matching("");
    assert_eq!(empty, matching("   "));
    assert_eq!(empty.len(), MAX_RESULTS);
    for text in ["😀", "👍", "❤️", "✅", "🚀", "🎉"] {
        assert!(empty.iter().any(|entry| entry.text == text));
    }
    assert_eq!(matching("face").len(), MAX_RESULTS);
}
