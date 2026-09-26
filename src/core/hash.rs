//! Small hashing helpers shared by features that need a stable string id.

use sha2::{Digest, Sha256};
use std::fmt::Write;

/// Lowercase hexadecimal SHA-256 digest of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest.iter() {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
