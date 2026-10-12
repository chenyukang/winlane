//! Keeping the Mac's music app from launching by itself.
//!
//! Music and iTunes open themselves when Bluetooth headphones reconnect, when a
//! media key is pressed, or when a web page asks for them. Blocking means
//! noticing that launch and quitting the app again right away, which is what
//! noTunes does; the user's own launches are a separate decision and are blocked
//! the same way, exactly like noTunes behaves.

/// Applications whose automatic launch is undone while the setting is on.
pub const BLOCKED_BUNDLE_IDS: [&str; 2] = ["com.apple.Music", "com.apple.iTunes"];

/// Whether this launch should be undone right away. Bundle identifiers are
/// matched exactly: a helper process of the same app has its own identifier and
/// must not be touched.
pub fn blocks(bundle_id: &str, enabled: bool) -> bool {
    enabled && BLOCKED_BUNDLE_IDS.contains(&bundle_id)
}
