use super::*;
use crate::macos::platform::preferences as preference_store;

// AX window IDs reserve the top bit (see accessibility::element_id). These
// process-local slots preserve activation order until a new app exposes a window.
pub(super) fn application_visit(pid: i32) -> u64 {
    (1 << 63) | u64::from(pid as u32)
}

fn is_application_visit(id: u64) -> bool {
    id & (1 << 63) != 0
}

impl Delegate {
    /// Keep the shortcut tap's idea of the frontmost application current. Key
    /// remaps leave the applications they name alone, and the tap callback runs
    /// inside event dispatch, so it cannot ask AppKit itself.
    pub(super) fn publish_remap_frontmost(&self, bundle_id: Option<String>) {
        if let Some(tap) = self.ivars().shortcut_tap.borrow().as_ref() {
            tap.set_frontmost_app(bundle_id);
        }
    }

    /// Publish the frontmost application now, for the moments right after a tap
    /// was installed and nothing has reported a switch yet.
    pub(super) fn publish_current_frontmost(&self) {
        self.publish_remap_frontmost(
            NSWorkspace::sharedWorkspace()
                .frontmostApplication()
                .and_then(|app| app.bundleIdentifier().map(|id| id.to_string())),
        );
    }

    pub(super) fn track_frontmost(&self) {
        if self.ivars().demo.get() {
            return;
        }
        let Some(app) = NSWorkspace::sharedWorkspace().frontmostApplication() else {
            return;
        };
        self.track_application(&app);
    }

    pub(super) fn track_application(&self, app: &NSRunningApplication) {
        if self.ivars().demo.get() {
            return;
        }
        // Key remaps need the real frontmost application, Winlane included
        // while its own panel holds focus, so this runs before the filters
        // below that skip Winlane and background applications.
        self.publish_remap_frontmost(app.bundleIdentifier().map(|id| id.to_string()));
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
        let cached = self.cached_application_window(pid);
        // Record the visit before another activation can replace the AX query.
        if let Some(id) = cached {
            self.request_focus(pid, Some(id));
        } else {
            // The targeted discovery already reads focus. Do not issue a second
            // competing AXFocusedWindow query for the same newly activated app.
            self.ivars().focus_receiver.take();
            self.ivars().focus_pid.set(pid);
            self.remember_window(application_visit(pid));
            self.discover_application_window(app);
        }
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

    pub(super) fn discover_application_window(&self, app: &NSRunningApplication) {
        let state = self.ivars();
        let pid = app.processIdentifier();
        if state
            .application_window_receivers
            .borrow()
            .iter()
            .any(|pending| pending.pid == pid)
        {
            return;
        }
        if let Some(identity) = catalog::app_identity(app) {
            state.identities.borrow_mut().insert(pid, identity);
        }
        let name = app
            .localizedName()
            .map(|name| name.to_string())
            .unwrap_or_default();
        let (tx, rx) = mpsc::channel();
        state
            .application_window_receivers
            .borrow_mut()
            .push(PendingApplicationWindow {
                pid,
                app: name.clone(),
                receiver: rx,
            });
        let wake = state.wake.get().unwrap().handle();
        std::thread::spawn(move || {
            let started = Instant::now();
            let window =
                objc2::rc::autoreleasepool(|_| accessibility::activated_window(pid, &name));
            crate::macos::platform::recency_trace::record_slow(
                "application-window",
                started,
                || format!("app_pid={pid} found={}", window.is_some()),
            );
            let _ = tx.send(window);
            wake.signal();
        });
    }

    pub(super) fn poll_application_windows(&self) {
        let state = self.ivars();
        let mut ready = Vec::new();
        state
            .application_window_receivers
            .borrow_mut()
            .retain(|pending| match pending.receiver.try_recv() {
                Ok(window) => {
                    if let Some(window) = window {
                        ready.push(window);
                    }
                    false
                }
                Err(TryRecvError::Disconnected) => false,
                Err(TryRecvError::Empty) => true,
            });
        let changed = !ready.is_empty();
        let waiting = state.mode.get() == Some(PanelMode::Switch)
            && state
                .switch_selection
                .borrow()
                .as_ref()
                .is_some_and(|s| s.selected().is_none());
        for (window, server_id) in ready {
            if state.closed_windows.borrow().contains(&window.id)
                || (!state.config.borrow().include_minimized && window.minimized)
            {
                continue;
            }
            if let Some(server_id) = server_id {
                state
                    .window_server_ids
                    .borrow_mut()
                    .insert(window.id, server_id);
            }
            if state.receiver.borrow().is_some() {
                state.window_additions.borrow_mut().push(window.clone());
            }
            let held = state.mode.get() == Some(PanelMode::Switch)
                && state
                    .switch_selection
                    .borrow()
                    .as_ref()
                    .is_some_and(|s| s.selected().is_some());
            let mut windows = if held {
                state
                    .deferred_windows
                    .take()
                    .unwrap_or_else(|| state.windows.borrow().clone())
            } else {
                state.windows.borrow().clone()
            };
            if let Some(existing) = windows.iter_mut().find(|existing| existing.id == window.id) {
                *existing = window;
            } else {
                windows.push(window);
            }
            if held {
                state.deferred_windows.replace(Some(windows));
            } else {
                self.install_windows(windows);
            }
        }
        // Once a gesture has a selection, it continues to own its frozen order.
        let frozen = state.mode.get() == Some(PanelMode::Switch)
            && state
                .switch_selection
                .borrow()
                .as_ref()
                .is_some_and(|s| s.selected().is_some());
        if (changed || waiting)
            && !frozen
            && !state.preparing_panel.get()
            && state.mode.get().is_some()
            && !self.scoped_search()
        {
            self.filter_preserving(self.selected_result());
            self.prepare_switch_selection();
            self.commit_switch_if_ready();
        }
    }

    pub(super) fn waiting_for_application_window(&self) -> bool {
        let state = self.ivars();
        let Some(selection) = state.switch_selection.borrow().as_ref().cloned() else {
            return false;
        };
        if selection.selected().is_some() {
            return false;
        }
        let recent = state.recency.borrow();
        let windows = state.windows.borrow();
        let pending = state.application_window_receivers.borrow();
        let mut projected = windows.clone();
        for &slot in recent.iter().filter(|id| is_application_visit(**id)) {
            let pid = slot as u32 as i32;
            if windows.iter().any(|window| window.pid == pid) {
                continue;
            }
            let read = pending.iter().find(|read| read.pid == pid);
            if !state.loading.get() && read.is_none() {
                continue;
            }
            projected.push(WindowInfo {
                id: slot,
                pid,
                app: read
                    .map(|read| read.app.clone())
                    .or_else(|| {
                        state
                            .identities
                            .borrow()
                            .get(&pid)
                            .map(|app| app.english_name.clone())
                    })
                    .unwrap_or_default(),
                title: String::new(),
                minimized: false,
            });
        }
        if projected.len() == windows.len() {
            return false;
        }
        // These are projection-only entries, never published or sent to AX.
        // Wait only if their eventual insertion changes this gesture's target.
        let config = state.config.borrow();
        let projected_matches = visible_matches(
            &projected,
            "",
            None,
            &config,
            None,
            &recent,
            state.previous_pid.get(),
        );
        if config.sort != winlane::core::config::SortOrder::Recent {
            return projected_matches
                .iter()
                .any(|&i| is_application_visit(projected[i].id));
        }
        let target = |windows: &[WindowInfo], matches: &[usize]| {
            let anchor = if let Some(id) = state.switch_anchor.get() {
                matches.iter().position(|&i| windows[i].id == id)
            } else {
                state
                    .previous_window
                    .get()
                    .and_then(|id| matches.iter().position(|&i| windows[i].id == id))
                    .or_else(|| {
                        matches
                            .iter()
                            .position(|&i| windows[i].pid == state.previous_pid.get())
                    })
            };
            let mut preview = selection.clone();
            preview.install(matches.len(), anchor);
            preview.selected().map(|i| windows[matches[i]].id)
        };
        target(&projected, &projected_matches) != target(&windows, &state.matches.borrow())
    }

    pub(super) fn resolve_application_visits(&self) {
        let state = self.ivars();
        let replacements: HashMap<_, _> = state
            .recency
            .borrow()
            .iter()
            .copied()
            .filter(|id| is_application_visit(*id))
            .filter_map(|slot| {
                self.cached_application_window(slot as u32 as i32)
                    .map(|id| (slot, id))
            })
            .collect();
        let resolve = |recent: &mut Vec<u64>| {
            let mut seen = HashSet::new();
            recent.retain_mut(|id| {
                if let Some(window) = replacements.get(id) {
                    *id = *window;
                }
                seen.insert(*id)
            });
        };
        resolve(&mut state.recency.borrow_mut());
        // An exact query can restore its baseline later; resolve that too.
        if let Some(pending) = state.focus_receiver.borrow_mut().as_mut() {
            resolve(&mut pending.recency);
        }
    }

    /// A completed full scan is also the fallback for apps with no readable
    /// focused/main window. Stop waiting for slots it could not resolve.
    pub(super) fn prune_application_visits(&self) {
        let state = self.ivars();
        let keep = |id: &u64| {
            !is_application_visit(*id)
                || state
                    .application_window_receivers
                    .borrow()
                    .iter()
                    .any(|pending| application_visit(pending.pid) == *id)
        };
        state.recency.borrow_mut().retain(keep);
        if let Some(pending) = state.focus_receiver.borrow_mut().as_mut() {
            pending.recency.retain(keep);
        }
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
        self.ivars().auto_appclose.borrow().cancel();
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
            self.resolve_application_visits();
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
        self.ivars().auto_appclose.borrow().cancel();
        // Auto AppClose counts idle time from the last visit to a window.
        if !is_application_visit(id) {
            self.ivars().auto_appclose.borrow_mut().mark_active(id);
        }
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
            let recent: Vec<_> = self
                .ivars()
                .recency
                .borrow()
                .iter()
                .copied()
                .filter(|id| !is_application_visit(*id))
                .collect();
            preference_store::save_recency(&recent, defaults);
        }
    }
}
