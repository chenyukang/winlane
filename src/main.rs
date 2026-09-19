#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(target_os = "macos")]
mod macos;

fn main() {
    #[cfg(target_os = "macos")]
    macos::app::run();
    #[cfg(not(target_os = "macos"))]
    eprintln!("Winlane requires macOS 14 or later.");
}
