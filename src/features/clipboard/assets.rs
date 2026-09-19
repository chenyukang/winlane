use crate::features::clipboard::{ImageInfo, MAX_IMAGE_BYTES};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug)]
pub struct SessionFiles {
    root: PathBuf,
}
#[derive(Debug)]
pub struct ImageAsset {
    pub image: PathBuf,
    pub thumbnail: PathBuf,
    _session: Arc<SessionFiles>,
}
impl Drop for ImageAsset {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.image);
        let _ = fs::remove_file(&self.thumbnail);
    }
}
impl Drop for SessionFiles {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn nonce() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}
impl SessionFiles {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            root: std::env::temp_dir().join(format!("winlane-images-{}", nonce())),
        })
    }
    fn asset(self: &Arc<Self>, extension: &str) -> io::Result<Arc<ImageAsset>> {
        private_directory(&self.root)?;
        let key = nonce();
        Ok(Arc::new(ImageAsset {
            image: self.root.join(format!("{key}.{extension}")),
            thumbnail: self.root.join(format!("{key}.thumb.png")),
            _session: self.clone(),
        }))
    }
    pub fn create(
        self: &Arc<Self>,
        info: &ImageInfo,
        bytes: &[u8],
        thumbnail: &[u8],
    ) -> io::Result<Arc<ImageAsset>> {
        if !info.valid()
            || bytes.len() != info.bytes
            || digest(bytes) != info.key
            || thumbnail.len() > 128 * 1024
        {
            return Err(io::Error::other("Invalid clipboard image"));
        }
        let asset = self.asset(info.format.extension())?;
        private_write(&asset.image, bytes)?;
        private_write(&asset.thumbnail, thumbnail)?;
        Ok(asset)
    }
    pub fn restore(self: &Arc<Self>, root: &Path, info: &ImageInfo) -> io::Result<Arc<ImageAsset>> {
        if !info.valid() {
            return Err(io::Error::other("Invalid clipboard image"));
        }
        let asset = self.asset(info.format.extension())?;
        let image = root.join(info.filename());
        let thumbnail = root.join(info.thumbnail_filename());
        let metadata = fs::symlink_metadata(&image)?;
        if !metadata.is_file() || metadata.len() != info.bytes as u64 {
            return Err(io::Error::other("Missing clipboard image"));
        }
        link_or_copy(&image, &asset.image)?;
        if fs::symlink_metadata(&thumbnail)
            .is_ok_and(|meta| meta.is_file() && meta.len() <= 128 * 1024)
        {
            link_or_copy(&thumbnail, &asset.thumbnail)?;
        }
        Ok(asset)
    }
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn image_bytes(info: &ImageInfo) -> io::Result<Vec<u8>> {
    let asset = info
        .asset
        .as_ref()
        .ok_or_else(|| io::Error::other("Clipboard image is unavailable"))?;
    let mut bytes = Vec::new();
    fs::File::open(&asset.image)?
        .take(MAX_IMAGE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() != info.bytes || bytes.len() > MAX_IMAGE_BYTES || digest(&bytes) != info.key {
        return Err(io::Error::other("Clipboard image changed or is invalid"));
    }
    Ok(bytes)
}
pub fn private_directory(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
pub fn private_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}
fn link_or_copy(from: &Path, to: &Path) -> io::Result<()> {
    let temporary = to.with_extension(format!("{}.tmp", nonce()));
    let result = (|| {
        if fs::hard_link(from, &temporary).is_err() {
            fs::copy(from, &temporary)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
        }
        fs::rename(&temporary, to)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn persist(root: &Path, info: &ImageInfo) -> io::Result<()> {
    if !info.valid() {
        return Err(io::Error::other("Invalid clipboard image"));
    }
    let asset = info
        .asset
        .as_ref()
        .ok_or_else(|| io::Error::other("Clipboard image is unavailable"))?;
    private_directory(root)?;
    for (source, target) in [
        (&asset.image, root.join(info.filename())),
        (&asset.thumbnail, root.join(info.thumbnail_filename())),
    ] {
        if !target.exists() && source.exists() {
            link_or_copy(source, &target)?;
        }
    }
    Ok(())
}
pub fn prune(root: &Path, keep: &std::collections::HashSet<String>) -> io::Result<()> {
    let files = match fs::read_dir(root) {
        Ok(files) => files,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for file in files {
        let file = file?;
        let name = file.file_name().to_string_lossy().into_owned();
        let owned = name
            .get(..64)
            .is_some_and(|key| key.bytes().all(|ch| ch.is_ascii_hexdigit()))
            && matches!(name.get(64..), Some(".png" | ".tiff" | ".thumb.png"));
        if owned && !keep.contains(&name) {
            fs::remove_file(file.path())?;
        }
    }
    Ok(())
}

#[cfg(unix)]
pub fn cleanup_stale_sessions(root: &Path) {
    use std::os::unix::fs::MetadataExt;
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
        fn getuid() -> u32;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name
            .to_str()
            .and_then(|name| name.strip_prefix("winlane-images-"))
        else {
            continue;
        };
        let parts: Vec<_> = name.split('-').collect();
        if parts.len() != 3
            || !parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|ch| ch.is_ascii_digit()))
        {
            continue;
        }
        let Ok(pid) = parts[0].parse::<i32>() else {
            continue;
        };
        if pid <= 0 {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !metadata.is_dir() || metadata.uid() != unsafe { getuid() } {
            continue;
        }
        // Signal zero only checks existence. Remove our own stale sessions, never active ones.
        if unsafe { kill(pid, 0) } == -1 && io::Error::last_os_error().raw_os_error() == Some(3) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}
