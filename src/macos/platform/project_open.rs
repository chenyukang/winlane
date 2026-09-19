use crate::macos::platform::main_wake::WakeHandle;
use block2::RcBlock;
use objc2::rc::{Retained, autoreleasepool};
use objc2_app_kit::{NSRunningApplication, NSWorkspace, NSWorkspaceOpenConfiguration};
use objc2_foundation::{NSArray, NSError, NSString, NSURL};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};
use winlane::features::projects::Project;
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
    target: Project,
}

pub struct OpenedProject {
    pub pid: i32,
    pub window: Option<u64>,
}

pub struct PendingProjectOpen {
    pub receiver: Receiver<Result<OpenedProject, String>>,
    pub origin_pid: i32,
    pub target_seen: bool,
    cancelled: Arc<AtomicBool>,
}

impl Drop for PendingProjectOpen {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
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
            target: project.clone(),
        })
    }

    pub fn open(self, origin_pid: i32, wake: WakeHandle) -> PendingProjectOpen {
        let configuration = NSWorkspaceOpenConfiguration::configuration();
        configuration.setActivates(true);
        configuration.setCreatesNewApplicationInstance(false);
        let (tx, rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let project = self.target;
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
                    let tx = tx.clone();
                    let wake = wake.clone();
                    let cancelled = worker_cancelled.clone();
                    let project = project.clone();
                    std::thread::spawn(move || {
                        let result = result.map(|pid| OpenedProject {
                            pid,
                            window: wait_for_window(pid, &project, &cancelled),
                        });
                        if !cancelled.load(Ordering::Relaxed) {
                            let _ = tx.send(result);
                            wake.signal();
                        }
                    });
                });
            });
        NSWorkspace::sharedWorkspace()
            .openURLs_withApplicationAtURL_configuration_completionHandler(
                &NSArray::from_retained_slice(&[self.project]),
                &self.application,
                &configuration,
                Some(&completion),
            );
        PendingProjectOpen {
            receiver: rx,
            origin_pid,
            target_seen: false,
            cancelled,
        }
    }
}

fn wait_for_window(pid: i32, project: &Project, cancelled: &AtomicBool) -> Option<u64> {
    if !super::accessibility::is_trusted() {
        return None;
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut ready = winlane::features::projects::focus::ReadyWindow::default();
    while Instant::now() < deadline && !cancelled.load(Ordering::Relaxed) {
        let candidate = winlane::features::projects::focus::target_window(
            project,
            &super::accessibility::project_windows(pid),
        );
        // Electron publishes intermediate windows while restoring a project.
        // Require the same identified window on two separate observations.
        if let Some(window) = ready.observe(candidate) {
            return Some(window);
        }
        std::thread::sleep(Duration::from_millis(120));
    }
    None
}

#[cfg(test)]
#[path = "../../../tests/native/project_open.rs"]
pub(crate) mod tests;
