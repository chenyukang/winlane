use crate::macos::platform::main_wake::WakeHandle;
use block2::RcBlock;
use objc2::rc::{Retained, autoreleasepool};
use objc2_app_kit::{NSRunningApplication, NSWorkspace, NSWorkspaceOpenConfiguration};
use objc2_foundation::{NSArray, NSError, NSString, NSURL};
use std::sync::mpsc::{self, Receiver};
use winlane::{tr, trf};

fn application_url() -> Option<Retained<NSURL>> {
    NSWorkspace::sharedWorkspace()
        .URLForApplicationWithBundleIdentifier(&NSString::from_str("com.google.Chrome"))
}

pub struct PreparedPage {
    pub(crate) url: Retained<NSURL>,
    application: Retained<NSURL>,
}

impl PreparedPage {
    pub fn new(address: &str) -> Result<Self, String> {
        if !winlane::features::open_url::is_web_url(address) {
            return Err(tr!("网址无效。", "Invalid web URL.").into());
        }
        let url = NSURL::URLWithString(&NSString::from_str(address))
            .filter(|url| url.host().is_some_and(|host| !host.is_empty()))
            .ok_or_else(|| tr!("网址无效。", "Invalid web URL.").to_owned())?;
        let application = application_url().ok_or_else(|| {
            tr!(
                "找不到 Google Chrome，请先安装应用。",
                "Google Chrome was not found. Install it first."
            )
            .to_owned()
        })?;
        Ok(Self { url, application })
    }

    pub fn open(self, wake: WakeHandle) -> Receiver<Result<i32, String>> {
        let configuration = NSWorkspaceOpenConfiguration::configuration();
        configuration.setActivates(true);
        configuration.setCreatesNewApplicationInstance(false);
        let (tx, rx) = mpsc::channel();
        let completion =
            RcBlock::new(move |app: *mut NSRunningApplication, error: *mut NSError| {
                autoreleasepool(|_| {
                    // SAFETY: AppKit keeps these nullable callback arguments alive for the call.
                    let result = unsafe {
                        if let Some(error) = error.as_ref() {
                            Err(trf!(
                                "无法打开网址：{}",
                                "Could not open URL: {}",
                                error.localizedDescription()
                            ))
                        } else if let Some(app) = app.as_ref() {
                            Ok(app.processIdentifier())
                        } else {
                            Err(tr!(
                                "Chrome 未完成启动，请重试。",
                                "Chrome did not finish launching. Try again."
                            )
                            .into())
                        }
                    };
                    let _ = tx.send(result);
                    wake.signal();
                });
            });
        NSWorkspace::sharedWorkspace()
            .openURLs_withApplicationAtURL_configuration_completionHandler(
                &NSArray::from_retained_slice(&[self.url]),
                &self.application,
                &configuration,
                Some(&completion),
            );
        rx
    }
}
