use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
const FILES: [&str; 3] = ["History", "History-wal", "History-journal"];

#[derive(PartialEq, Eq)]
struct Stamp {
    size: u64,
    modified: SystemTime,
}

fn stamp(path: &Path) -> io::Result<Option<Stamp>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
                return Err(io::Error::other("History file is unavailable or too large"));
            }
            Ok(Some(Stamp {
                size: metadata.len(),
                modified: metadata.modified()?,
            }))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) struct Snapshot(tempfile::TempDir);

impl Snapshot {
    pub(super) fn new(profile: &Path) -> io::Result<Self> {
        // Chrome can hold an exclusive SQLite lock for its whole session. Copy its
        // journals too, so SQLite can recover a committed view in our private copy.
        for _ in 0..3 {
            let directory = tempfile::Builder::new()
                .prefix("winlane-history-")
                .tempdir()?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))?;
            }
            let before = FILES
                .iter()
                .map(|name| stamp(&profile.join(name)))
                .collect::<io::Result<Vec<_>>>()?;
            if before[0].is_none() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "History unavailable",
                ));
            }
            let mut copied = true;
            for (name, expected) in FILES.iter().zip(&before) {
                if let Some(expected) = expected {
                    let result = (|| {
                        let mut input = File::open(profile.join(name))?.take(MAX_FILE_BYTES + 1);
                        let mut output = File::create(directory.path().join(name))?;
                        io::copy(&mut input, &mut output)
                    })();
                    match result {
                        Ok(size) if size == expected.size => {}
                        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                        _ => {
                            copied = false;
                            break;
                        }
                    }
                }
            }
            let after = FILES
                .iter()
                .map(|name| stamp(&profile.join(name)))
                .collect::<io::Result<Vec<_>>>()?;
            if copied && before == after {
                return Ok(Self(directory));
            }
        }
        Err(io::Error::other(
            "Chrome history changed while reading; retry",
        ))
    }

    pub(super) fn database(&self) -> PathBuf {
        self.0.path().join("History")
    }
}
