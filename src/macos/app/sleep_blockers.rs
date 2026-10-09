//! Which applications are holding sleep back, and the offer to close them
//! automatically once they have been idle for a while.
//!
//! The machine's power assertions are read on a background thread, because
//! `pmset` takes tens of milliseconds. Results are kept so the keep-awake panel
//! and the Auto AppClose page can name the app that keeps the Mac awake.

use super::*;
use crate::macos::platform::{applications::target_at_url, sleep_blockers as reader};
use block2::RcBlock;
use winlane::core::config::ApplicationTarget;

/// How often the assertions are read while a relevant window is open.
const CHECK_INTERVAL: Duration = Duration::from_secs(5);

/// An application holding at least one sleep assertion.
#[derive(Clone, Debug)]
pub(super) struct BlockingApp {
    pub(super) target: ApplicationTarget,
    /// `Name (reason)` for each assertion, for example `WeChat (Video Wake Lock)`.
    pub(super) reasons: Vec<String>,
}

#[derive(Default)]
pub(super) struct Blockers {
    apps: Vec<BlockingApp>,
    /// Apps the user has already been asked about, so the question is asked once.
    asked: HashSet<String>,
    receiver: Option<Receiver<Vec<BlockingApp>>>,
    next_check: Option<Instant>,
}

impl Blockers {
    /// `微信、X 正在阻止休眠`, or nothing when no app holds sleep back.
    pub(super) fn hint(&self) -> Option<String> {
        if self.apps.is_empty() {
            return None;
        }
        let names = self
            .apps
            .iter()
            .map(|app| app.target.name.as_str())
            .collect::<Vec<_>>()
            .join("、");
        Some(trf!("{} 正在阻止休眠", "{} is holding sleep back", names))
    }
}

/// Resolve the assertions that hold sleep back to running applications. System
/// processes have no application, so the power manager's own assertions drop out.
fn resolve(own_pid: i32) -> Vec<BlockingApp> {
    let mut apps: Vec<BlockingApp> = Vec::new();
    for blocker in reader::read()
        .iter()
        .filter(|blocker| blocker.prevents_sleep())
    {
        if blocker.pid == own_pid {
            continue;
        }
        let Some(application) =
            NSRunningApplication::runningApplicationWithProcessIdentifier(blocker.pid)
        else {
            continue;
        };
        let Some(url) = application.bundleURL() else {
            continue;
        };
        let Ok(target) = target_at_url(&url) else {
            continue;
        };
        let reason = blocker.label();
        match apps
            .iter_mut()
            .find(|entry| entry.target.bundle_id == target.bundle_id)
        {
            Some(entry) => entry.reasons.push(reason),
            None => apps.push(BlockingApp {
                target,
                reasons: vec![reason],
            }),
        }
    }
    apps
}

impl Delegate {
    pub(super) fn poll_sleep_blockers(&self) {
        let state = self.ivars();
        let watched = self.settings_window().is_some() || self.searching_keep_awake();
        if !watched && state.sleep_blockers.borrow().receiver.is_none() {
            return;
        }
        let mut blockers = state.sleep_blockers.borrow_mut();
        if let Some(receiver) = blockers.receiver.take() {
            match receiver.try_recv() {
                Ok(apps) => blockers.apps = apps,
                Err(TryRecvError::Empty) => blockers.receiver = Some(receiver),
                Err(TryRecvError::Disconnected) => {}
            }
        }
        let now = Instant::now();
        if watched
            && blockers.receiver.is_none()
            && blockers.next_check.is_none_or(|next| now >= next)
        {
            blockers.next_check = Some(now + CHECK_INTERVAL);
            let (tx, rx) = mpsc::channel();
            blockers.receiver = Some(rx);
            let wake = state.wake.get().unwrap().handle();
            let own_pid = std::process::id() as i32;
            std::thread::spawn(move || {
                let _ = tx.send(resolve(own_pid));
                wake.signal();
            });
        }
        // Ask once per app, and only while Settings can show the question.
        let pending: Vec<BlockingApp> = blockers
            .apps
            .iter()
            .filter(|app| !blockers.asked.contains(&app.target.bundle_id))
            .cloned()
            .collect();
        if pending.is_empty() || state.saving_settings.get() {
            return;
        }
        // Only ask while the window is really in front of the user; hidden test
        // windows must not raise a sheet or change settings.
        let Some(settings) = self
            .settings_window()
            .filter(|settings| settings.window.isVisible())
        else {
            return;
        };
        for app in &pending {
            blockers.asked.insert(app.target.bundle_id.clone());
        }
        drop(blockers);
        self.ask_about_sleep_blockers(&settings, &pending);
    }

    fn ask_about_sleep_blockers(&self, settings: &SettingsWindow, pending: &[BlockingApp]) {
        let names = pending
            .iter()
            .map(|app| app.target.name.as_str())
            .collect::<Vec<_>>()
            .join("、");
        let alert = NSAlert::new(settings.window.mtm());
        alert.setMessageText(&NSString::from_str(tr!(
            "有应用正在阻止 Mac 休眠",
            "An app is keeping your Mac awake"
        )));
        alert.setInformativeText(&NSString::from_str(&trf!(
            "{} 正在阻止屏幕或系统休眠，所以显示器不会自动关闭、Mac 也不会挂起。\n\n要给这些应用添加“闲置 {} 分钟后自动关闭窗口”的规则吗？选择“保持现状”不会做任何改动，设置页的提示仍会保留。",
            "{} is holding sleep back, so the display does not turn off and the Mac does not sleep.\n\nAdd a rule that closes that app's windows after {} idle minutes? Choosing “Leave it as is” changes nothing, and the note in Settings stays.",
            names,
            winlane::features::auto_appclose::SUGGESTED_IDLE_MINUTES
        )));
        alert.addButtonWithTitle(&NSString::from_str(tr!("添加闲置关闭", "Add idle closing")));
        alert.addButtonWithTitle(&NSString::from_str(tr!("保持现状", "Leave it as is")));
        let weak = Weak::new(self);
        let pending = pending.to_vec();
        let completion = RcBlock::new(move |response: isize| {
            if response != NSAlertFirstButtonReturn {
                return;
            }
            let Some(delegate) = weak.load() else {
                return;
            };
            delegate.add_idle_closing_for(&pending);
        });
        alert.beginSheetModalForWindow_completionHandler(&settings.window, Some(&completion));
    }

    /// Add the suggested rules and turn Auto AppClose on, since a rule does
    /// nothing while the feature is off. Saved settings on failure.
    fn add_idle_closing_for(&self, apps: &[BlockingApp]) {
        let Some(settings) = self.settings_window() else {
            return;
        };
        let targets: Vec<ApplicationTarget> = apps.iter().map(|app| app.target.clone()).collect();
        let mut config = self.ivars().config.borrow().clone();
        let rules = winlane::features::sleep_blockers::idle_rules_for(
            &targets,
            &config.auto_appclose.rules,
            winlane::features::auto_appclose::SUGGESTED_IDLE_MINUTES,
        );
        if rules.is_empty() {
            return;
        }
        let added = rules.len();
        config.auto_appclose.rules.extend(rules);
        config.auto_appclose.enabled = true;
        if let Err(error) = config.validate() {
            settings.report(&trf!("未保存：{}", "Not saved: {}", error), true);
            return;
        }
        // The Auto AppClose page must show the rules the save reads back.
        settings.fill(&config);
        match self.apply_config(config) {
            Ok(()) => settings.report(
                &trf!(
                    "已为 {} 个应用添加闲置 {} 分钟后关闭，并开启自动关闭窗口。",
                    "Added idle closing after {} minutes for {} apps and turned Auto AppClose on.",
                    winlane::features::auto_appclose::SUGGESTED_IDLE_MINUTES,
                    added
                ),
                false,
            ),
            Err(error) => settings.report(&trf!("未保存：{}", "Not saved: {}", error), true),
        }
    }
}
