use super::*;
use crate::macos::platform::{
    accessibility::cleanup,
    logging::{Level, record},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use winlane::features::auto_cleanup::{Planner, Settings, Target};

enum Job {
    Scan(Receiver<cleanup::Scan>, u64),
    Close(Receiver<cleanup::CloseResult>, Target, u64, Arc<AtomicBool>),
}

#[derive(Default)]
pub(super) struct State {
    settings: Settings,
    planner: Planner,
    generation: u64,
    job: Option<Job>,
    next_scan: Option<Instant>,
    started: Option<Instant>,
}

impl State {
    pub(super) fn cancel(&self) {
        if let Some(Job::Close(_, _, _, token)) = &self.job {
            token.store(true, Ordering::Release);
        }
    }
    pub(super) fn configure(&mut self, settings: &Settings) {
        if &self.settings == settings {
            return;
        }
        self.cancel();
        let rules_changed =
            self.settings.enabled != settings.enabled || self.settings.rules != settings.rules;
        if rules_changed {
            self.generation = self.generation.wrapping_add(1);
            self.planner = Planner::default();
        }
        self.settings = settings.clone();
        // An interval edit must not retry a close still awaiting the user's save decision.
        self.next_scan = if rules_changed {
            None
        } else {
            Some(Instant::now() + settings.interval())
        };
        record(Level::Info, "auto-cleanup", "configuration", || {
            format!(
                "enabled={} rules={} interval_secs={}",
                settings.enabled,
                settings.rules.len(),
                settings.interval_secs,
            )
        });
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl Delegate {
    pub(super) fn poll_auto_cleanup(&self) {
        let app = self.ivars();
        let mut state = app.auto_cleanup.borrow_mut();
        state.configure(&app.config.borrow().auto_cleanup);
        let busy = app.demo.get()
            || app.mode.get().is_some()
            || app.focus_receiver.borrow().is_some()
            || app.project_open.borrow().is_some()
            || app.launch_receiver.borrow().is_some()
            || self.settings_window().is_some_and(|s| s.window.isVisible());
        if busy {
            state.cancel();
        }
        let now = Instant::now();
        let now_ms = now
            .duration_since(*state.started.get_or_insert(now))
            .as_millis() as u64;
        let mut closed = Vec::new();
        if let Some(job) = state.job.take() {
            match job {
                Job::Scan(rx, generation) => match rx.try_recv() {
                    Ok(scan) if generation == state.generation => {
                        for target in state.planner.confirm_closed(&scan.closed) {
                            record(Level::Info, "auto-cleanup", "closed", || {
                                cleanup::details(&target)
                            });
                            closed.push(target.window.id);
                        }
                        state.planner.observe(&scan.apps, now_ms);
                        if !busy
                            && let Some(target) = state.planner.candidate(
                                &state.settings,
                                &scan.apps,
                                &app.recency.borrow(),
                                now_ms,
                            )
                        {
                            let rule = state
                                .settings
                                .rules
                                .iter()
                                .find(|r| r.application.bundle_id == target.bundle_id)
                                .unwrap()
                                .clone();
                            let token = Arc::new(AtomicBool::new(false));
                            let worker_token = token.clone();
                            let worker_target = target.clone();
                            let (tx, rx) = mpsc::channel();
                            let wake = app.wake.get().unwrap().handle();
                            std::thread::spawn(move || {
                                objc2::rc::autoreleasepool(|_| {
                                    let result =
                                        cleanup::close(&worker_target, &rule, &worker_token);
                                    let _ = tx.send(result);
                                    wake.signal();
                                })
                            });
                            state.job = Some(Job::Close(rx, target, generation, token));
                        }
                    }
                    Err(TryRecvError::Empty) => state.job = Some(Job::Scan(rx, generation)),
                    _ => {}
                },
                Job::Close(rx, target, generation, token) => match rx.try_recv() {
                    Ok(cleanup::CloseResult::Requested | cleanup::CloseResult::Failed)
                    | Err(TryRecvError::Disconnected) => {
                        if generation == state.generation {
                            state.planner.attempted(target);
                        }
                    }
                    Err(TryRecvError::Empty) => {
                        state.job = Some(Job::Close(rx, target, generation, token))
                    }
                    Ok(cleanup::CloseResult::Skipped) => {}
                },
            }
        }
        if !busy
            && state.settings.enabled
            && !state.settings.rules.is_empty()
            && state.job.is_none()
            && state.next_scan.is_none_or(|next| now >= next)
        {
            state.next_scan = Some(now + state.settings.interval());
            let rules = state.settings.rules.clone();
            let pending = state.planner.pending().cloned().collect::<Vec<_>>();
            let (tx, rx) = mpsc::channel();
            state.job = Some(Job::Scan(rx, state.generation));
            let wake = app.wake.get().unwrap().handle();
            std::thread::spawn(move || {
                objc2::rc::autoreleasepool(|_| {
                    let _ = tx.send(cleanup::scan(&rules, &pending));
                    wake.signal();
                })
            });
        }
        let pending: Vec<_> = state
            .planner
            .pending()
            .filter_map(|p| {
                state
                    .settings
                    .rules
                    .iter()
                    .find(|r| r.application.bundle_id == p.bundle_id)
                    .map(|r| r.application.name.as_str())
            })
            .collect();
        if let Some(settings) = self.settings_window() {
            let status = if !state.settings.enabled {
                tr!("已关闭，保留现有规则。", "Off. Your rules are kept.").into()
            } else if !pending.is_empty() {
                trf!(
                    "已暂停 {} 的清理，等待窗口关闭。可重新开关自动清理以重试。",
                    "Cleanup paused for {} until the window closes. Toggle Auto Cleanup to retry.",
                    pending.join(", ")
                )
            } else if !accessibility::is_trusted() {
                tr!("需要辅助功能权限。", "Accessibility access is required.").into()
            } else {
                tr!(
                    "后台运行；编辑设置和使用 Winlane 时暂停。",
                    "Running in the background; paused while using Winlane."
                )
                .into()
            };
            settings.update_cleanup_status(&status);
        }
        drop(state);
        self.remove_closed_windows(&closed);
    }
}

#[cfg(test)]
#[path = "../../../tests/native/app/auto_cleanup.rs"]
pub(crate) mod tests;
