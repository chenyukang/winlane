use objc2_app_kit::{NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication};
use objc2_foundation::{MainThreadMarker, NSLocale, NSString, NSUserDefaults, ns_string};
use objc2_service_management::SMAppService;
use winlane::core::aliases::Aliases;
use winlane::core::config::{Appearance, Config};
use winlane::core::i18n::{self, Language};

pub fn load_aliases() -> Result<Aliases, String> {
    let defaults = NSUserDefaults::standardUserDefaults();
    defaults
        .stringForKey(ns_string!("WinlaneAliasesV2"))
        .or_else(|| defaults.stringForKey(ns_string!("WinlaneAliasesV1")))
        .map(|value| Aliases::from_json(&value.to_string()))
        .unwrap_or_else(|| Ok(Aliases::default()))
}

pub fn save_aliases(aliases: &Aliases) {
    let json = NSString::from_str(&aliases.to_json());
    // SAFETY: The value is a property-list string, stored separately from shortcut settings.
    unsafe {
        NSUserDefaults::standardUserDefaults()
            .setObject_forKey(Some(&json), ns_string!("WinlaneAliasesV2"));
    }
}

pub(crate) const RECENT_WINDOW_LIMIT: usize = 128;

pub(crate) fn load_recency(defaults: &NSUserDefaults) -> Vec<u64> {
    let mut recent = defaults
        .stringForKey(ns_string!("WinlaneRecentWindowsV1"))
        .and_then(|value| serde_json::from_str::<Vec<u64>>(&value.to_string()).ok())
        .unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    recent.retain(|id| seen.insert(*id));
    recent.truncate(RECENT_WINDOW_LIMIT);
    recent
}

pub(crate) fn save_recency(recent: &[u64], defaults: &NSUserDefaults) {
    let recent = &recent[..recent.len().min(RECENT_WINDOW_LIMIT)];
    let json = NSString::from_str(&serde_json::to_string(recent).unwrap());
    // SAFETY: Only window IDs are stored, separately from settings and aliases.
    unsafe {
        defaults.setObject_forKey(Some(&json), ns_string!("WinlaneRecentWindowsV1"));
    }
}

fn storage_key() -> &'static NSString {
    ns_string!("WindowlanePreferencesV1")
}

pub fn load() -> Result<Config, String> {
    NSUserDefaults::standardUserDefaults()
        .stringForKey(storage_key())
        .map(|value| Config::from_json(&value.to_string()))
        .unwrap_or_else(|| Ok(Config::default()))
}

pub fn save(config: &Config, defaults: &NSUserDefaults) -> Result<(), String> {
    let json = NSString::from_str(&config.to_json()?);
    // SAFETY: NSString is an accepted property-list value. One key stores the whole validated configuration.
    unsafe { defaults.setObject_forKey(Some(&json), storage_key()) };
    Ok(())
}

pub fn apply_language(language: Language) -> bool {
    let languages: Vec<_> = NSLocale::preferredLanguages()
        .iter()
        .map(|value| value.to_string())
        .collect();
    i18n::set_locale(language.resolve(languages.iter().map(String::as_str)))
}

pub fn apply_appearance(config: &Config, mtm: MainThreadMarker) {
    let appearance = match config.appearance {
        Appearance::System => None,
        // SAFETY: These immutable AppKit constants are available on all supported systems.
        Appearance::Light => NSAppearance::appearanceNamed(unsafe { NSAppearanceNameAqua }),
        Appearance::Dark => NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }),
    };
    NSApplication::sharedApplication(mtm).setAppearance(appearance.as_deref());
}

pub fn manage_login() {
    // SAFETY: Opens the documented settings pane in response to the user's button click.
    unsafe { SMAppService::openSystemSettingsLoginItems() };
}
