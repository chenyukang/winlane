use super::*;
use objc2_app_kit::{NSRunningApplication, NSWorkspace};
use objc2_foundation::NSString;
use std::sync::atomic::{AtomicBool, Ordering};
use winlane::features::auto_appclose::{Rule, Snapshot, Target, Window};

pub struct Scan {
    pub apps: Vec<Snapshot>,
    pub closed: Vec<u64>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CloseResult {
    Requested,
    Skipped,
    Failed,
}

pub fn scan(rules: &[Rule], pending: &[Target]) -> Scan {
    let Some(inventory) = Inventory::try_read().filter(|_| is_trusted()) else {
        return Scan {
            apps: Vec::new(),
            closed: Vec::new(),
        };
    };
    let closed = pending
        .iter()
        .filter_map(|target| {
            target
                .window
                .server_id
                .filter(|id| !inventory.contains(*id))
                .map(|_| target.window.id)
        })
        .collect();
    let apps = rules
        .iter()
        .filter_map(|rule| {
            snapshot(rule, &inventory)
                .map_err(|error| {
                    record(Level::Debug, "auto-appclose", "scan-skipped", || {
                        format!("bundle={} reason={error}", rule.application.bundle_id)
                    });
                })
                .ok()
        })
        .collect();
    Scan { apps, closed }
}

fn snapshot(rule: &Rule, inventory: &Inventory) -> Result<Snapshot, String> {
    let bundle = NSString::from_str(&rule.application.bundle_id);
    let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&bundle);
    let frontmost = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|app| app.processIdentifier());
    let mut windows = Vec::new();
    let mut seen = HashSet::new();
    for app in apps {
        let pid = app.processIdentifier();
        let application = Element::application(pid).ok_or("app unavailable")?;
        let focused = if frontmost == Some(pid) {
            Some(
                application
                    .element("AXFocusedWindow")
                    .map_err(|_| "focus unavailable")?,
            )
        } else {
            None
        };
        if focused.as_ref().is_some_and(|w| {
            w.boolean("AXModal") == Some(true)
                || w.string("AXSubrole").as_deref() == Some("AXDialog")
        }) {
            return Err("modal window active".into());
        }
        // Unlike display-only discovery, an incomplete AX read must never trigger automatic window closing.
        let published = application
            .windows()
            .map_err(|_| "window list unavailable")?;
        for window in complete_windows(published, pid, inventory) {
            if window.string("AXSubrole").as_deref() != Some("AXStandardWindow") {
                continue;
            }
            let id = window.id(pid);
            if !seen.insert(id) {
                continue;
            }
            let server_id = window.server_id();
            if server_id.is_some_and(|id| !inventory.contains(id)) {
                continue;
            }
            let sheets = match window.elements("AXSheets") {
                Ok(sheets) => !sheets.is_empty(),
                Err(AX_ATTRIBUTE_UNSUPPORTED | AX_NO_VALUE | AX_NOT_IMPLEMENTED) => false,
                Err(_) => return Err("sheet state unavailable".into()),
            };
            if sheets || window.boolean("AXModal") == Some(true) {
                return Err("save sheet or modal window present".into());
            }
            windows.push(Window {
                id,
                pid,
                server_id,
                protected: focused
                    .as_ref()
                    .is_some_and(|focused| focused.id(pid) == id)
                    || window.boolean("AXEdited") == Some(true),
            });
        }
    }
    Ok(Snapshot {
        bundle_id: rule.application.bundle_id.clone(),
        windows,
    })
}

pub fn close(target: &Target, rule: &Rule, cancelled: &AtomicBool) -> CloseResult {
    let attempt = || -> Result<bool, String> {
        if cancelled.load(Ordering::Acquire) || !is_trusted() {
            return Ok(false);
        }
        let inventory = Inventory::try_read().ok_or("window inventory unavailable")?;
        let current = snapshot(rule, &inventory)?;
        if current.windows.len() <= usize::from(rule.max_windows)
            || !current.windows.iter().any(|w| {
                w.id == target.window.id
                    && w.pid == target.window.pid
                    && w.server_id == target.window.server_id
                    && !w.protected
            })
        {
            return Ok(false);
        }
        let window = find_window(target.window.pid, target.window.id)?;
        if window.server_id() != target.window.server_id {
            return Ok(false);
        }
        let button = window
            .element("AXCloseButton")
            .map_err(|_| "close button unavailable")?;
        if button.boolean("AXEnabled") == Some(false) {
            return Err("close button disabled".into());
        }
        let front = NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .ok_or("frontmost app unavailable")?;
        if front.processIdentifier() == std::process::id() as i32 {
            return Ok(false);
        }
        if front.processIdentifier() == target.window.pid {
            let application = Element::application(target.window.pid).ok_or("app unavailable")?;
            let focused = application
                .element("AXFocusedWindow")
                .map_err(|_| "focus unavailable")?;
            if focused.id(target.window.pid) == target.window.id
                || focused.boolean("AXModal") == Some(true)
                || focused.string("AXSubrole").as_deref() == Some("AXDialog")
            {
                return Ok(false);
            }
        }
        if cancelled.load(Ordering::Acquire) {
            return Ok(false);
        }
        record(Level::Info, "auto-appclose", "close-request", || {
            details(target)
        });
        let action = CFString::new("AXPress");
        // SAFETY: Both owned AX objects remain live. This invokes the app's normal close button;
        // it neither kills the process nor answers any unsaved-content prompt.
        let status =
            unsafe { AXUIElementPerformAction(button.raw(), action.as_concrete_TypeRef()) };
        if status != AX_SUCCESS {
            return Err(format!("AXPress error={status}"));
        }
        Ok(true)
    };
    match attempt() {
        Ok(true) => CloseResult::Requested,
        Ok(false) => CloseResult::Skipped,
        Err(error) => {
            record(Level::Warn, "auto-appclose", "close-unconfirmed", || {
                format!("{} reason={error}", details(target))
            });
            CloseResult::Failed
        }
    }
}

pub fn details(target: &Target) -> String {
    format!(
        "bundle={} app_pid={} window={} server_id={:?} limit={}",
        target.bundle_id,
        target.window.pid,
        target.window.id,
        target.window.server_id,
        target.max_windows
    )
}
