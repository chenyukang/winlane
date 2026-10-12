//! Quit Music or iTunes again as soon as it launches by itself.
//!
//! macOS starts Music when Bluetooth headphones reconnect, when a media key is
//! pressed, or when a page asks for playback. While the setting is on, a
//! launched Music or iTunes is asked to quit — a normal quit request, never a
//! force kill — so only the app the user is not using goes away.

use super::*;

impl Delegate {
    pub(super) fn quit_blocked_music(&self) {
        let enabled = self.ivars().config.borrow().block_music;
        // The setting is off by default; the check costs nothing when it is.
        if !enabled {
            return;
        }
        for application in NSWorkspace::sharedWorkspace().runningApplications() {
            let Some(bundle_id) = application.bundleIdentifier() else {
                continue;
            };
            let bundle_id = bundle_id.to_string();
            if !winlane::features::music_block::blocks(&bundle_id, enabled) {
                continue;
            }
            // A helper process has its own identifier, so this only ever names
            // the app itself. An app already quitting returns false.
            if application.terminate() {
                crate::macos::platform::logging::record(
                    crate::macos::platform::logging::Level::Info,
                    "music-block",
                    "quit-requested",
                    || format!("bundle={bundle_id}"),
                );
            }
        }
    }
}
