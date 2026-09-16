#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(target_os = "macos")]
mod accessibility;
#[cfg(target_os = "macos")]
mod app;
#[cfg(target_os = "macos")]
mod app_shortcuts;
#[cfg(target_os = "macos")]
mod input_source;
#[cfg(target_os = "macos")]
mod installed_apps;
#[cfg(target_os = "macos")]
mod main_wake;
#[cfg(target_os = "macos")]
mod settings;
#[cfg(target_os = "macos")]
mod shortcut_tap;
#[cfg(target_os = "macos")]
mod window_server;

fn main() {
    #[cfg(target_os = "macos")]
    app::run();
    #[cfg(not(target_os = "macos"))]
    eprintln!("Winlane requires macOS 14 or later.");
}
