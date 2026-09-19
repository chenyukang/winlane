use super::*;
use crate::macos::platform::preferences as preference_store;

impl Delegate {
    pub(super) fn track_frontmost(&self) {
        if self.ivars().demo.get() {
            return;
        }
        let Some(app) = NSWorkspace::sharedWorkspace().frontmostApplication() else {
            return;
        };
        let pid = app.processIdentifier();
        crate::macos::platform::recency_trace::record("frontmost", || {
            format!(
                "app_pid={pid} policy={:?} panel_key={} mode={:?}",
                app.activationPolicy(),
                self.any_panel_key(),
                self.ivars().mode.get(),
            )
        });
        if pid == std::process::id() as i32
            || app.activationPolicy() != NSApplicationActivationPolicy::Regular
        {
            return;
        }
        self.cancel_project_open_if_switched(
            pid,
            app.bundleIdentifier()
                .is_some_and(|id| id.to_string() == "com.microsoft.VSCode"),
        );
        self.check_window_liveness();
        self.request_focus(pid, None);
        if self
            .ivars()
            .focus_observer
            .borrow()
            .as_ref()
            .is_some_and(|observer| observer.pid == pid)
        {
            return;
        }
        let weak = Weak::new(self);
        let observer = crate::macos::platform::focus_observer::FocusObserver::new(
            pid,
            self.mtm(),
            move |event| {
                if let Some(delegate) = weak.load()
                    && !delegate.ivars().demo.get()
                {
                    match event {
                        crate::macos::platform::focus_observer::WindowEvent::Destroyed(id) => {
                            delegate.remove_closed_windows(&[id]);
                        }
                        crate::macos::platform::focus_observer::WindowEvent::FocusChanged => {
                            delegate.check_window_liveness();
                            if NSWorkspace::sharedWorkspace()
                                .frontmostApplication()
                                .is_some_and(|app| app.processIdentifier() == pid)
                            {
                                delegate.request_focus(pid, None);
                            }
                        }
                    }
                }
            },
        );
        crate::macos::platform::recency_trace::record("observer", || {
            format!("app_pid={pid} registered={}", observer.is_some())
        });
        self.ivars().focus_observer.replace(observer);
    }

    pub(super) fn capture_panel_origin(&self) {
        if self.ivars().demo.get() || self.any_panel_key() {
            return;
        }
        if let Some(front) = NSWorkspace::sharedWorkspace().frontmostApplication()
            && front.processIdentifier() != std::process::id() as i32
        {
            let pid = front.processIdentifier();
            let window = self.cached_application_window(pid);
            self.ivars().previous_pid.set(pid);
            self.ivars().previous_window.set(window);
            self.request_focus(pid, window);
        }
    }

    pub(super) fn request_focus(&self, pid: i32, provisional: Option<u64>) {
        let state = self.ivars();
        crate::macos::platform::recency_trace::record("focus-request", || {
            format!(
                "app_pid={pid} provisional={provisional:?} revision={} pending={:?} recent={:?}",
                state.focus_revision.get(),
                state
                    .focus_receiver
                    .borrow()
                    .as_ref()
                    .map(|p| (p.pid, p.revision)),
                state.recency.borrow(),
            )
        });
        // Preserve the history before a provisional cached window was moved
        // forward. A later exact result must not invent a visit to its sibling.
        let recency = state
            .focus_receiver
            .borrow()
            .as_ref()
            .filter(|pending| pending.pid == pid && pending.revision == state.focus_revision.get())
            .map_or_else(
                || state.recency.borrow().clone(),
                |pending| pending.recency.clone(),
            );
        if let Some(id) = provisional {
            self.remember_window(id);
        }
        let (tx, rx) = mpsc::channel();
        state.focus_pid.set(pid);
        state.focus_receiver.replace(Some(PendingFocus {
            receiver: rx,
            pid,
            revision: state.focus_revision.get(),
            recency,
        }));
        let wake = state.wake.get().unwrap().handle();
        std::thread::spawn(move || {
            let _ = tx.send(accessibility::focused_window(pid, FocusRead::Background));
            wake.signal();
        });
    }

    pub(super) fn poll_focus(&self) {
        let state = self.ivars();
        let result = state
            .focus_receiver
            .borrow()
            .as_ref()
            .map(|pending| (pending.receiver.try_recv(), pending.pid, pending.revision));
        let (result, pid, revision) = match result {
            None | Some((Err(TryRecvError::Empty), _, _)) => return,
            Some(result) => result,
        };
        let pending = state.focus_receiver.take().unwrap();
        crate::macos::platform::recency_trace::record("focus-result", || {
            format!(
                "app_pid={pid} result={result:?} revision={revision} current_revision={} focus_pid={} baseline={:?} recent={:?}",
                state.focus_revision.get(),
                state.focus_pid.get(),
                pending.recency,
                state.recency.borrow(),
            )
        });
        if revision != state.focus_revision.get() || pid != state.focus_pid.get() {
            return;
        }
        let window = result
            .ok()
            .flatten()
            .or_else(|| self.cached_application_window(pid));
        crate::macos::platform::recency_trace::record("focus-apply", || {
            format!(
                "app_pid={pid} window={window:?} known={}",
                window.is_some_and(|id| state.windows.borrow().iter().any(|w| w.id == id)),
            )
        });
        if let Some(id) = window {
            state.recency.replace(pending.recency);
            self.remember_window(id);
            if state.previous_pid.get() == pid {
                state.previous_window.set(Some(id));
            }
            // A held switch gesture owns its snapshot. Search can update the
            // order, but must keep the same selected result as data arrives.
            if state.mode.get() == Some(PanelMode::Search) && !self.scoped_search() {
                self.filter_preserving(self.selected_result());
            }
        }
    }

    pub(super) fn remember_frontmost_window(&self) {
        if self.ivars().demo.get() || self.any_panel_key() {
            return;
        }
        if let Some(front) = NSWorkspace::sharedWorkspace().frontmostApplication()
            && front.processIdentifier() != std::process::id() as i32
        {
            let pid = front.processIdentifier();
            self.ivars().previous_pid.set(pid);
            self.ivars()
                .previous_window
                .set(self.remember_application(pid));
        }
    }

    pub(super) fn remember_application(&self, pid: i32) -> Option<u64> {
        crate::macos::platform::recency_trace::record("capture", || {
            format!(
                "app_pid={pid} cached={:?}",
                self.cached_application_window(pid)
            )
        });
        let id = accessibility::focused_window(pid, FocusRead::Immediate)
            .or_else(|| self.cached_application_window(pid))?;
        self.remember_window(id);
        Some(id)
    }

    pub(super) fn cached_application_window(&self, pid: i32) -> Option<u64> {
        let windows = self.ivars().windows.borrow();
        let recent = self.ivars().recency.borrow();
        windows
            .iter()
            .filter(|window| window.pid == pid)
            .min_by_key(|window| {
                (
                    window.minimized,
                    recent
                        .iter()
                        .position(|id| *id == window.id)
                        .unwrap_or(usize::MAX),
                )
            })
            .map(|window| window.id)
    }

    #[track_caller]
    pub(super) fn remember_window(&self, id: u64) {
        let caller = std::panic::Location::caller();
        crate::macos::platform::recency_trace::record("visit", || {
            format!(
                "id={id} app_pid={:?} caller={} revision={} before={:?}",
                self.ivars()
                    .windows
                    .borrow()
                    .iter()
                    .find(|w| w.id == id)
                    .map(|w| w.pid),
                caller,
                self.ivars().focus_revision.get(),
                self.ivars().recency.borrow(),
            )
        });
        self.ivars()
            .focus_revision
            .set(self.ivars().focus_revision.get().wrapping_add(1));
        let mut recent = self.ivars().recency.borrow_mut();
        recent.retain(|previous| *previous != id);
        recent.insert(0, id);
        recent.truncate(preference_store::RECENT_WINDOW_LIMIT);
    }

    pub(super) fn restore_recency(&self, defaults: Retained<NSUserDefaults>) {
        self.ivars()
            .recency
            .replace(preference_store::load_recency(&defaults));
        let _ = self.ivars().recency_store.set(defaults);
        crate::macos::platform::recency_trace::record("restore", || {
            format!("recent={:?}", self.ivars().recency.borrow())
        });
    }

    pub(super) fn save_recency(&self) {
        crate::macos::platform::recency_trace::record("exit-save", || {
            format!("recent={:?}", self.ivars().recency.borrow())
        });
        if let Some(defaults) = self.ivars().recency_store.get() {
            preference_store::save_recency(&self.ivars().recency.borrow(), defaults);
        }
    }
}
