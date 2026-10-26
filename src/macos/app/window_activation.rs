use super::*;
use crate::macos::platform::{applications, logging};
use logging::Level;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use winlane::core::activation::{
    ACTIVATION_TIMEOUT, Activation, Decision, Observation, VERIFY_INTERVAL, WindowState,
};

type LaunchReceiver = Receiver<Result<i32, applications::ActivationError>>;

pub(super) struct PendingWindowActivation {
    pub(super) id: u64,
    pub(super) started: Instant,
    pub(super) session: u64,
    pub(super) window: WindowInfo,
    pub(super) bundle: String,
    pub(super) query: String,
    pub(super) policy: Activation,
    pub(super) current: Arc<AtomicBool>,
    pub(super) last_observation: Option<Observation>,
    probe: Option<(Instant, Receiver<WindowState>)>,
    raise: Option<Receiver<Result<(), String>>>,
    launch: Option<LaunchReceiver>,
    next_probe: Instant,
    last_action: Instant,
}

impl PendingWindowActivation {
    pub(super) fn new(
        id: u64,
        session: u64,
        window: WindowInfo,
        bundle: String,
        query: String,
        origin: Option<i32>,
        server: Option<u32>,
    ) -> Self {
        let started = Instant::now();
        Self {
            id,
            started,
            session,
            policy: Activation::new(
                window.pid,
                window.id,
                server,
                origin,
                std::process::id() as i32,
            ),
            window,
            bundle,
            query,
            current: Arc::new(AtomicBool::new(true)),
            last_observation: None,
            probe: None,
            raise: None,
            launch: None,
            next_probe: started,
            last_action: started,
        }
    }

    fn base(&self) -> Value {
        json!({"test_mode":cfg!(test),"request_id":self.id,"session":self.session,"target_pid":self.window.pid,
            "bundle_id":self.bundle,"window_id":self.window.id,"server_id":self.policy.server_id,
            "origin_pid":self.policy.origin_pid,"phase":if self.policy.fallback {"launchservices"} else {"cooperative"},
            "raise_attempt":self.policy.raises})
    }

    pub(super) fn trace(&self, event: &str, level: Level, mut extra: Value) {
        if let Some(o) = self.last_observation {
            let fields = json!({"front_pid":o.front_pid,"active":o.active,"running":o.running,
                "focused_id":o.window.focused_id,"focused_server_id":o.window.focused_server_id,
                "on_screen":o.window.on_screen,"alive":o.window.alive,"ax_error":o.window.ax_error});
            for (key, value) in fields.as_object().unwrap() {
                if extra.get(key).is_none() {
                    extra[key] = value.clone();
                }
            }
        }
        trace(self.base(), self.started, event, level, extra);
    }
}

fn trace(mut base: Value, started: Instant, event: &str, level: Level, extra: Value) {
    logging::record(level, "window-activation", event, || {
        for (key, value) in extra.as_object().unwrap() {
            base[key] = value.clone();
        }
        base["elapsed_ms"] = json!(started.elapsed().as_secs_f64() * 1000.0);
        base.to_string()
    });
}

impl Delegate {
    pub(super) fn request_window_activation(&self, window: WindowInfo, app: &NSRunningApplication) {
        self.cancel_window_activation("superseded");
        let state = self.ivars();
        let id = state.activation_sequence.get().wrapping_add(1);
        state.activation_sequence.set(id);
        let front = NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|app| app.processIdentifier());
        // If a settings window had activated Winlane, closing it can return
        // focus to the saved origin. That is still the expected handoff.
        let origin = front
            .filter(|pid| *pid != std::process::id() as i32)
            .or_else(|| (state.previous_pid.get() > 0).then_some(state.previous_pid.get()));
        let pending = PendingWindowActivation::new(
            id,
            state.session.get(),
            window.clone(),
            app.bundleIdentifier()
                .map(|id| id.to_string())
                .unwrap_or_default(),
            state.query.borrow().trim().to_lowercase(),
            origin,
            state.window_server_ids.borrow().get(&window.id).copied(),
        );
        pending.trace(
            "requested",
            Level::Info,
            json!({"was_minimized":window.minimized,"front_pid":front}),
        );
        // Release the nonactivating panel and its field editor before asking
        // another app to own keyboard focus. End-session must not mean success.
        self.end_session();
        state.window_activation.replace(Some(pending));
        let unhide_allowed = app.unhide();
        let allowed = self.activate_app(app);
        if let Some(pending) = state
            .window_activation
            .borrow_mut()
            .as_mut()
            .filter(|p| p.id == id)
        {
            pending.policy.request_allowed = allowed;
            pending.trace(
                "cooperative-result",
                Level::Info,
                json!({"allowed":allowed,"unhide_allowed":unhide_allowed}),
            );
        }
        if !state
            .window_activation
            .borrow()
            .as_ref()
            .is_some_and(|p| p.id == id)
        {
            return;
        }
        // Only alive during a handoff; no new idle polling is introduced.
        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                VERIFY_INTERVAL.as_secs_f64(),
                self,
                sel!(verifyWindowActivation:),
                None,
                true,
            )
        };
        timer.setTolerance(0.01);
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
        state.window_activation_timer.replace(Some(timer));
        self.poll_window_activation();
    }

    pub(super) fn cancel_window_activation(&self, reason: &str) {
        if let Some(pending) = self.ivars().window_activation.take() {
            pending.current.store(false, Ordering::Release);
            let front = NSWorkspace::sharedWorkspace()
                .frontmostApplication()
                .map(|app| app.processIdentifier());
            pending.trace(
                "cancelled",
                Level::Info,
                json!({"reason":reason,"front_pid":front}),
            );
        }
        if let Some(timer) = self.ivars().window_activation_timer.take() {
            timer.invalidate();
        }
    }

    pub(super) fn cancel_interrupted_window_activation(&self) {
        if self.ivars().window_activation.borrow().is_none() {
            return;
        }
        let front = NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|app| app.processIdentifier());
        let interrupted = self
            .ivars()
            .window_activation
            .borrow_mut()
            .as_mut()
            .is_some_and(|p| p.policy.interrupted(front));
        if interrupted {
            self.cancel_window_activation("foreground-changed");
        }
    }

    pub(super) fn poll_window_activation(&self) {
        self.cancel_interrupted_window_activation();
        let Some((pid, bundle)) = self
            .ivars()
            .window_activation
            .borrow()
            .as_ref()
            .map(|p| (p.window.pid, p.bundle.clone()))
        else {
            return;
        };
        let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid);
        let front = NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|app| app.processIdentifier());
        let mut observation = Observation {
            front_pid: front,
            active: app.as_ref().is_some_and(|a| a.isActive()),
            running: app.as_ref().is_some_and(|a| {
                !a.isTerminated()
                    && (bundle.is_empty()
                        || a.bundleIdentifier()
                            .is_some_and(|id| id.to_string() == bundle))
            }),
            window: WindowState::default(),
        };
        let (id, guard_decision) = {
            let mut slot = self.ivars().window_activation.borrow_mut();
            let Some(pending) = slot.as_mut() else {
                return;
            };
            if let Some(last) = pending.last_observation {
                observation.window = last.window;
            }
            (
                pending.id,
                pending
                    .policy
                    .decide(pending.started.elapsed(), observation, true),
            )
        };
        if matches!(guard_decision, Decision::Cancelled | Decision::Failed(_)) {
            self.finish_window_activation(id, guard_decision, observation);
            return;
        }
        let launch_result = {
            let slot = self.ivars().window_activation.borrow();
            slot.as_ref()
                .and_then(|p| p.launch.as_ref())
                .map(|rx| rx.try_recv())
        };
        if let Some(result) = launch_result {
            match result {
                Ok(Ok(actual_pid)) if actual_pid == pid => {
                    if let Some(pending) = self.ivars().window_activation.borrow_mut().as_mut() {
                        pending.launch.take();
                        pending.policy.fallback_complete = true;
                        pending.last_action = Instant::now();
                        pending.trace(
                            "launchservices-result",
                            Level::Info,
                            json!({"ok":true,"returned_pid":actual_pid}),
                        );
                    }
                }
                Ok(Err(error)) => {
                    self.fail_window_activation(
                        id,
                        "launchservices-error",
                        json!({"error_domain":error.domain,"error_code":error.code}),
                    );
                    return;
                }
                Ok(Ok(actual_pid)) => {
                    self.fail_window_activation(
                        id,
                        "process-changed",
                        json!({"returned_pid":actual_pid}),
                    );
                    return;
                }
                Err(TryRecvError::Disconnected) => {
                    self.fail_window_activation(id, "launchservices-disconnected", json!({}));
                    return;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        let raised = {
            let slot = self.ivars().window_activation.borrow();
            slot.as_ref()
                .and_then(|p| p.raise.as_ref())
                .map(|rx| rx.try_recv())
        };
        if let Some(result) = raised
            && !matches!(result, Err(TryRecvError::Empty))
            && let Some(pending) = self.ivars().window_activation.borrow_mut().as_mut()
        {
            pending.raise.take();
            pending.last_action = Instant::now();
            pending.trace(
                "raise-result",
                Level::Info,
                json!({"ok":matches!(result, Ok(Ok(())))}),
            );
        }
        let sample = {
            let mut slot = self.ivars().window_activation.borrow_mut();
            let Some(pending) = slot.as_mut() else {
                return;
            };
            match pending
                .probe
                .as_ref()
                .map(|(started, rx)| (*started, rx.try_recv()))
            {
                Some((started, Ok(window))) => {
                    pending.probe.take();
                    (started >= pending.last_action).then_some(window)
                }
                Some((_, Err(TryRecvError::Disconnected))) => {
                    pending.probe.take();
                    None
                }
                _ => None,
            }
        };
        if let Some(window) = sample {
            // Use current AppKit state, not the state at worker launch. A late
            // AX response from the target cannot undo a user's newer app switch.
            observation.front_pid = NSWorkspace::sharedWorkspace()
                .frontmostApplication()
                .map(|app| app.processIdentifier());
            observation.active = app.as_ref().is_some_and(|a| a.isActive());
            observation.running = app.as_ref().is_some_and(|a| {
                !a.isTerminated()
                    && (bundle.is_empty()
                        || a.bundleIdentifier()
                            .is_some_and(|id| id.to_string() == bundle))
            });
            observation.window = window;
            self.observe_window_activation(id, observation);
        }
        self.start_activation_probe(id);
    }

    pub(super) fn observe_window_activation(&self, id: u64, observation: Observation) {
        let decision = {
            let mut slot = self.ivars().window_activation.borrow_mut();
            let Some(pending) = slot.as_mut().filter(|p| p.id == id) else {
                return;
            };
            if pending.last_observation != Some(observation) {
                pending.last_observation = Some(observation);
                pending.trace("observed", Level::Info, json!({}));
            }
            pending.policy.decide(
                pending.started.elapsed(),
                observation,
                pending.raise.is_some() || pending.launch.is_some(),
            )
        };
        match decision {
            Decision::Raise => self.raise_activation_window(id),
            Decision::Reopen => self.reopen_activation_app(id),
            Decision::Wait => {}
            _ => self.finish_window_activation(id, decision, observation),
        }
    }

    fn start_activation_probe(&self, id: u64) {
        let task = {
            let mut slot = self.ivars().window_activation.borrow_mut();
            let Some(p) = slot.as_mut().filter(|p| p.id == id) else {
                return;
            };
            let now = Instant::now();
            if p.probe.is_some() || p.raise.is_some() || now < p.next_probe {
                return;
            }
            let (tx, rx) = mpsc::channel();
            p.probe = Some((now, rx));
            p.next_probe = now + VERIFY_INTERVAL;
            (p.window.pid, p.policy.server_id, tx)
        };
        let wake = self.ivars().wake.get().unwrap().handle();
        std::thread::spawn(move || {
            let _ = task
                .2
                .send(accessibility::activation_window_state(task.0, task.1));
            wake.signal();
        });
    }

    fn raise_activation_window(&self, id: u64) {
        self.cancel_interrupted_window_activation();
        let pid = {
            let slot = self.ivars().window_activation.borrow();
            let Some(p) = slot.as_ref().filter(|p| p.id == id) else {
                return;
            };
            p.window.pid
        };
        let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) else {
            self.fail_window_activation(id, "app-exited", json!({}));
            return;
        };
        // Like a Dock click, restore ordered-out windows before correcting
        // focus. Keep retries inside the cancellable, bounded handoff.
        let allowed = self.activate_app(&app);
        if let Some(p) = self
            .ivars()
            .window_activation
            .borrow()
            .as_ref()
            .filter(|p| p.id == id)
        {
            p.trace(
                "cooperative-result",
                Level::Info,
                json!({"allowed":allowed,"reason":"show-window-retry"}),
            );
        }
        self.cancel_interrupted_window_activation();
        let task = {
            let mut slot = self.ivars().window_activation.borrow_mut();
            let Some(p) = slot.as_mut().filter(|p| p.id == id) else {
                return;
            };
            p.last_action = Instant::now();
            p.probe.take();
            p.trace("raise-requested", Level::Info, json!({}));
            let (tx, rx) = mpsc::channel();
            p.raise = Some(rx);
            (
                p.window.pid,
                p.window.id,
                p.current.clone(),
                p.started,
                p.base(),
                tx,
                p.policy.server_id,
            )
        };
        let wake = self.ivars().wake.get().unwrap().handle();
        std::thread::spawn(move || {
            let current =
                || task.2.load(Ordering::Acquire) && task.3.elapsed() < ACTIVATION_TIMEOUT;
            let result = accessibility::raise_window_guarded(
                task.0,
                task.1,
                task.6,
                &current,
                &|operation, status, code| {
                    trace(
                        task.4.clone(),
                        task.3,
                        "ax-step",
                        Level::Info,
                        json!({"operation":operation,"status":status,"ax_code":code,"request_current":current()}),
                    )
                },
            );
            let _ = task.5.send(result);
            wake.signal();
        });
    }

    fn reopen_activation_app(&self, id: u64) {
        self.cancel_interrupted_window_activation();
        let pid = {
            let slot = self.ivars().window_activation.borrow();
            let Some(p) = slot.as_ref().filter(|p| p.id == id) else {
                return;
            };
            p.trace("fallback-requested", Level::Info, json!({}));
            p.window.pid
        };
        let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) else {
            self.fail_window_activation(id, "app-exited", json!({}));
            return;
        };
        match applications::activate_running(&app, id, self.ivars().wake.get().unwrap().handle()) {
            Ok(receiver) => {
                if let Some(p) = self
                    .ivars()
                    .window_activation
                    .borrow_mut()
                    .as_mut()
                    .filter(|p| p.id == id)
                {
                    p.launch = Some(receiver);
                    p.last_action = Instant::now();
                    p.probe.take();
                }
            }
            Err(_) => self.fail_window_activation(id, "no-bundle-url", json!({})),
        }
    }

    pub(super) fn finish_window_activation(
        &self,
        id: u64,
        decision: Decision,
        observation: Observation,
    ) {
        if !self
            .ivars()
            .window_activation
            .borrow()
            .as_ref()
            .is_some_and(|p| p.id == id)
        {
            return;
        }
        match decision {
            Decision::Confirmed => {
                let Some(pending) = self.take_window_activation(id) else {
                    return;
                };
                let mut pending = pending;
                pending.last_observation = Some(observation);
                pending.trace(
                    "confirmed",
                    Level::Info,
                    json!({"stable_ms":pending.policy.stable_ms(pending.started.elapsed())}),
                );
                self.remember_window(pending.window.id);
                if !pending.query.is_empty() {
                    let mut preferences = self.ivars().preferences.borrow_mut();
                    if preferences.len() > 128 {
                        preferences.clear();
                    }
                    preferences.insert(pending.query, pending.window.id);
                }
            }
            Decision::Cancelled => self.cancel_window_activation("foreground-changed"),
            Decision::Failed(reason) => {
                if let Some(p) = self
                    .ivars()
                    .window_activation
                    .borrow_mut()
                    .as_mut()
                    .filter(|p| p.id == id)
                {
                    p.last_observation = Some(observation);
                }
                self.fail_window_activation(
                    id,
                    match reason {
                        winlane::core::activation::Failure::TimedOut => "timeout",
                        winlane::core::activation::Failure::AppExited => "app-exited",
                        winlane::core::activation::Failure::WindowClosed => "window-closed",
                    },
                    json!({}),
                );
            }
            _ => {}
        }
    }

    fn take_window_activation(&self, id: u64) -> Option<PendingWindowActivation> {
        if !self
            .ivars()
            .window_activation
            .borrow()
            .as_ref()
            .is_some_and(|p| p.id == id)
        {
            return None;
        }
        let pending = self.ivars().window_activation.take()?;
        pending.current.store(false, Ordering::Release);
        if let Some(timer) = self.ivars().window_activation_timer.take() {
            timer.invalidate();
        }
        Some(pending)
    }

    fn fail_window_activation(&self, id: u64, reason: &str, mut extra: Value) {
        let Some(pending) = self.take_window_activation(id) else {
            return;
        };
        extra["reason"] = json!(reason);
        pending.trace("failed", Level::Warn, extra);
        if reason == "window-closed" {
            self.remove_closed_windows(&[pending.window.id]);
        }
        // Do not reopen our UI over a newer user task or a new Winlane session.
        let front = NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|app| app.processIdentifier());
        if self.ivars().demo.get()
            || self.ivars().session.get() != pending.session
            || self.ivars().mode.get().is_some()
            || front.is_some_and(|pid| {
                Some(pid) != pending.policy.origin_pid
                    && pid != pending.window.pid
                    && pid != std::process::id() as i32
            })
        {
            return;
        }
        // Resume the shortcut router as well as the visible search UI, so a
        // failed handoff can be retried with the normal keyboard behavior.
        self.selection_failed(tr!(
            "未能确认目标窗口已切换到前台，请重试或查看日志。",
            "Could not confirm the selected window is in front. Retry or check the logs."
        ));
        self.present_panels();
    }
}
