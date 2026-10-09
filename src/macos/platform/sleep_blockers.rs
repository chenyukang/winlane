//! Read the power management assertions of the machine.
//!
//! `pmset -g assertions` prints which process holds which assertion and for how
//! long. It is read-only: it reports state and changes nothing.

use winlane::features::sleep_blockers::{Blocker, parse};

pub fn read() -> Vec<Blocker> {
    let Ok(output) = std::process::Command::new("/usr/bin/pmset")
        .args(["-g", "assertions"])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse(&String::from_utf8_lossy(&output.stdout))
}
