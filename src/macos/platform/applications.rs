use super::main_wake::WakeHandle;
use block2::RcBlock;
use objc2::rc::{Retained, autoreleasepool};
use objc2_app_kit::{NSRunningApplication, NSWorkspace, NSWorkspaceOpenConfiguration};
use objc2_foundation::{NSBundle, NSError, NSString, NSURL};
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use winlane::core::config::ApplicationTarget;
use winlane::{tr, trf};

pub fn target_at_url(url: &NSURL) -> Result<ApplicationTarget, String> {
    let path = url
        .path()
        .ok_or(tr!(
            "无法读取应用路径。",
            "The application path could not be read."
        ))?
        .to_string();
    let bundle = NSBundle::bundleWithURL(url)
        .ok_or(tr!("请选择一个 .app 应用。", "Choose an .app application."))?;
    let bundle_id = bundle
        .bundleIdentifier()
        .ok_or(tr!(
            "这个应用没有有效的应用标识。",
            "This app has no valid bundle identifier."
        ))?
        .to_string();
    let name = ["CFBundleDisplayName", "CFBundleName"]
        .into_iter()
        .find_map(|key| {
            bundle
                .objectForInfoDictionaryKey(&NSString::from_str(key))?
                .downcast_ref::<NSString>()
                .map(|name| name.to_string())
        })
        .unwrap_or_else(|| {
            Path::new(&path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
    let target = ApplicationTarget {
        bundle_id,
        path,
        name,
    };
    target.validate()?;
    if bundle.executableURL().is_none() {
        return Err(tr!("这个应用没有可运行的程序。", "This app has no executable.").into());
    }
    Ok(target)
}

pub fn resolve_application(target: &ApplicationTarget) -> Result<Retained<NSURL>, String> {
    target.validate()?;
    let saved = NSURL::fileURLWithPath(&NSString::from_str(&target.path));
    if target_at_url(&saved).is_ok_and(|found| found.bundle_id == target.bundle_id) {
        return Ok(saved);
    }
    if let Some(url) = NSWorkspace::sharedWorkspace()
        .URLForApplicationWithBundleIdentifier(&NSString::from_str(&target.bundle_id))
        && target_at_url(&url).is_ok_and(|found| found.bundle_id == target.bundle_id)
    {
        return Ok(url);
    }
    Err(trf!(
        "找不到 {}，应用可能已移动或卸载。",
        "Could not find {}. It may have been moved or uninstalled.",
        target.name
    ))
}

pub fn launch(
    target: &ApplicationTarget,
    wake: WakeHandle,
) -> Result<Receiver<Result<i32, String>>, String> {
    let url = resolve_application(target)?;
    let configuration = NSWorkspaceOpenConfiguration::configuration();
    configuration.setActivates(true);
    configuration.setCreatesNewApplicationInstance(false);
    configuration.setAddsToRecentItems(false);
    let (tx, rx) = mpsc::channel();
    let name = target.name.clone();
    let completion = RcBlock::new(move |app: *mut NSRunningApplication, error: *mut NSError| {
        autoreleasepool(|_| {
            // SAFETY: AppKit keeps these nullable callback arguments alive for the call.
            let result = unsafe {
                if let Some(error) = error.as_ref() {
                    Err(trf!(
                        "无法打开 {name}：{}",
                        "Could not open {name}: {}",
                        error.localizedDescription()
                    ))
                } else if let Some(app) = app.as_ref() {
                    Ok(app.processIdentifier())
                } else {
                    Err(trf!(
                        "无法打开 {name}，系统没有返回应用。",
                        "Could not open {name}. macOS did not return an application."
                    ))
                }
            };
            let _ = tx.send(result);
            wake.signal();
        });
    });
    NSWorkspace::sharedWorkspace().openApplicationAtURL_configuration_completionHandler(
        &url,
        &configuration,
        Some(&completion),
    );
    Ok(rx)
}
