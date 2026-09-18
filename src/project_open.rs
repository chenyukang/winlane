use crate::main_wake::WakeHandle;
use block2::RcBlock;
use objc2::rc::{Retained, autoreleasepool};
use objc2_app_kit::{NSRunningApplication, NSWorkspace, NSWorkspaceOpenConfiguration};
use objc2_foundation::{NSArray, NSError, NSString, NSURL};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use winlane::projects::Project;
use winlane::{tr, trf};

fn application_url() -> Option<Retained<NSURL>> {
    NSWorkspace::sharedWorkspace()
        .URLForApplicationWithBundleIdentifier(&NSString::from_str("com.microsoft.VSCode"))
}

pub fn application_path() -> Option<PathBuf> {
    application_url()?
        .path()
        .map(|path| PathBuf::from(path.to_string()))
}

pub struct PreparedProject {
    pub(crate) project: Retained<NSURL>,
    application: Retained<NSURL>,
}

impl PreparedProject {
    pub fn new(project: &Project) -> Result<Self, String> {
        project.validate_path()?;
        let application = application_url().ok_or_else(|| {
            tr!(
                "找不到 Visual Studio Code，请先安装应用。",
                "Visual Studio Code was not found. Install it first."
            )
            .to_owned()
        })?;
        Ok(Self {
            project: NSURL::fileURLWithPath(&NSString::from_str(&project.path.to_string_lossy())),
            application,
        })
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
                                "无法打开项目：{}",
                                "Could not open project: {}",
                                error.localizedDescription()
                            ))
                        } else if let Some(app) = app.as_ref() {
                            Ok(app.processIdentifier())
                        } else {
                            Err(tr!(
                                "VS Code 未完成启动，请重试。",
                                "VS Code did not finish launching. Try again."
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
                &NSArray::from_retained_slice(&[self.project]),
                &self.application,
                &configuration,
                Some(&completion),
            );
        rx
    }
}
