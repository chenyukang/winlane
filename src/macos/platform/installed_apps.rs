use std::collections::{HashSet, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use objc2::rc::autoreleasepool;
use objc2_foundation::{NSBundle, NSHomeDirectory, NSNumber, NSString, NSURL};
use winlane::core::app_catalog::InstalledApp;

pub fn application_roots() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/Applications"),
        PathBuf::from(NSHomeDirectory().to_string()).join("Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Library/CoreServices/Applications"),
        PathBuf::from("/System/Library/CoreServices/Finder.app"),
    ]
}

pub fn discover() -> Vec<InstalledApp> {
    autoreleasepool(|_| collect(&application_roots(), &indexed_paths()))
}

fn usable_path(path: &Path) -> bool {
    let system_app = path.starts_with("/System/Applications")
        || path.starts_with("/System/Cryptexes/App/System/Applications")
        || path.starts_with("/System/Library/CoreServices/Applications")
        || path == Path::new("/System/Library/CoreServices/Finder.app");
    let user_library = path
        .components()
        .map(|part| part.as_os_str())
        .collect::<Vec<_>>()
        .windows(3)
        .any(|parts| parts[0] == "Users" && parts[2] == "Library");
    path.is_absolute()
        && !path.starts_with("/Volumes")
        && (!path.starts_with("/System") || system_app)
        && !path.starts_with("/Library")
        && !user_library
        && !path
            .components()
            .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
        && !path
            .ancestors()
            .skip(1)
            .any(|part| part.extension().is_some_and(|ext| ext == "app"))
}

fn read_app(path: &Path) -> Option<InstalledApp> {
    if !usable_path(path) {
        return None;
    }
    let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?));
    let bundle = NSBundle::bundleWithURL(&url)?;
    let info = bundle.infoDictionary()?;
    if info
        .objectForKey(&NSString::from_str("LSBackgroundOnly"))
        .and_then(|value| value.downcast::<NSNumber>().ok())
        .is_some_and(|value| value.boolValue())
    {
        return None;
    }
    let package = info.objectForKey(&NSString::from_str("CFBundlePackageType"))?;
    if package.downcast_ref::<NSString>()?.to_string() != "APPL" {
        return None;
    }
    let target = crate::macos::platform::applications::target_at_url(&url).ok()?;
    if target.bundle_id == "app.windowlane.desktop" {
        return None;
    }
    let executable = bundle.executableURL()?.path()?;
    if !Path::new(&executable.to_string()).is_file() {
        return None;
    }
    let mut names = vec![
        target.name.clone(),
        path.file_stem()?.to_string_lossy().into_owned(),
    ];
    for key in ["CFBundleDisplayName", "CFBundleName"] {
        if let Some(name) = info
            .objectForKey(&NSString::from_str(key))
            .and_then(|value| value.downcast::<NSString>().ok())
        {
            names.push(name.to_string());
        }
    }
    names.sort();
    names.dedup();
    Some(InstalledApp { target, names })
}

fn collect(roots: &[PathBuf], indexed: &[PathBuf]) -> Vec<InstalledApp> {
    let mut paths = Vec::new();
    let mut visited = HashSet::new();
    let mut pending: VecDeque<_> = roots.iter().map(|root| (root.clone(), 0)).collect();
    while let Some((path, depth)) = pending.pop_front() {
        if !usable_path(&path) {
            continue;
        }
        let Ok(canonical) = path.canonicalize() else {
            continue;
        };
        if !visited.insert(canonical) {
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "app") {
            paths.push(path);
            continue;
        }
        // Application bundles are leaves: nested helpers must never become launch results.
        if depth >= 8
            || path.extension().is_some_and(|extension| {
                matches!(
                    extension.to_str(),
                    Some("bundle" | "framework" | "plugin" | "xpc" | "appex" | "kext")
                )
            })
        {
            continue;
        }
        if let Ok(entries) = path.read_dir() {
            let mut entries: Vec<_> = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .collect();
            entries.sort();
            pending.extend(
                entries
                    .into_iter()
                    .filter(|path| path.is_dir())
                    .map(|path| (path, depth + 1)),
            );
        }
    }
    paths.extend_from_slice(indexed);
    let mut identities = HashSet::new();
    let mut apps: Vec<_> = paths
        .iter()
        .filter_map(|path| autoreleasepool(|_| read_app(path)))
        .filter(|app| identities.insert(app.target.bundle_id.clone()))
        .collect();
    apps.sort_by(|a, b| {
        a.target
            .name
            .to_lowercase()
            .cmp(&b.target.name.to_lowercase())
            .then_with(|| a.target.bundle_id.cmp(&b.target.bundle_id))
    });
    apps
}

fn indexed_paths() -> Vec<PathBuf> {
    let Ok(mut child) = Command::new("/usr/bin/mdfind")
        .args(["-0", "kMDItemContentType == 'com.apple.application-bundle'"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Vec::new();
    };
    let stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.take(2 * 1024 * 1024).read_to_end(&mut bytes);
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    let mut paths: Vec<_> = reader
        .join()
        .unwrap_or_default()
        .split(|byte| *byte == 0)
        .filter_map(|bytes| std::str::from_utf8(bytes).ok())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .collect();
    paths.sort();
    paths
}

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../tests/native/installed_apps.rs"]
pub(crate) mod tests;
