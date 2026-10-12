use winlane::features::music_block::{BLOCKED_BUNDLE_IDS, blocks};

#[test]
fn blocking_covers_music_and_itunes_only_while_enabled() {
    for bundle in BLOCKED_BUNDLE_IDS {
        assert!(blocks(bundle, true), "{bundle} must be blocked");
        assert!(!blocks(bundle, false), "{bundle} must launch when off");
    }
}

#[test]
fn nothing_else_is_ever_blocked() {
    for bundle in [
        "com.apple.Safari",
        "com.spotify.client",
        "com.apple.Music.Helper",
        "com.apple.iTunesCloud",
        "",
        "COM.APPLE.MUSIC",
    ] {
        assert!(!blocks(bundle, true), "{bundle} must never be blocked");
    }
}
