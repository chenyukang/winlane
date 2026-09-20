#![allow(dead_code)]

#[cfg(target_os = "macos")]
#[path = "../src/macos/mod.rs"]
mod macos;

fn main() {
    #[cfg(target_os = "macos")]
    objc2::rc::autoreleasepool(|_| {
        winlane::core::i18n::set_locale(winlane::core::i18n::Locale::Chinese);
        if let Ok(expectation) = std::env::var("WINLANE_UPDATER_SMOKE") {
            crate::macos::platform::updater::tests::verify(&expectation);
        } else if std::env::var_os("WINLANE_LAUNCH_SMOKE").is_some() {
            crate::macos::ui::app_shortcuts::tests::verify_background_launch();
        } else if std::env::var_os("WINLANE_APP_SCAN").is_some() {
            let start = std::time::Instant::now();
            let apps = macos::platform::installed_apps::discover();
            if let Ok(query) = std::env::var("WINLANE_APP_SCAN") {
                let matches = winlane::core::app_catalog::matching_apps(
                    &apps,
                    &query,
                    &std::collections::HashSet::new(),
                    &[],
                    None,
                );
                for index in matches {
                    println!("Matched installed app: {:?}", apps[index]);
                }
            }
            assert!(!apps.is_empty());
            assert_eq!(
                apps.iter()
                    .map(|app| &app.target.bundle_id)
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                apps.len()
            );
            println!(
                "Installed application scan: {} apps in {:?}; no applications launched.",
                apps.len(),
                start.elapsed()
            );
        } else if let Ok(bundle) = std::env::var("WINLANE_SCAN_BUNDLE") {
            crate::macos::app::tests::diagnostics::inspect_window_discovery(&bundle);
        } else if std::env::var_os("WINLANE_BENCHMARK").is_some() {
            crate::macos::app::tests::diagnostics::benchmark_hidden_panels();
        } else {
            crate::macos::app::tests::verify_hidden_panels();
        }
    });
    #[cfg(not(target_os = "macos"))]
    println!("Native panel checks require macOS.");
}
