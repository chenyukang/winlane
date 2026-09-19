use super::*;
use crate::macos::platform::preferences as preference_store;

impl Delegate {
    pub(super) fn refresh(&self) {
        let state = self.ivars();
        if !accessibility::is_trusted() {
            state.windows.borrow_mut().clear();
            state.loading.set(false);
            self.filter();
            return;
        }
        self.check_window_liveness();
        if state.receiver.borrow().is_some() {
            self.render();
            return;
        }
        state.closed_windows.borrow_mut().clear();
        let identities = state.identities.borrow().clone();
        state.loading.set(true);
        let (tx, rx) = mpsc::channel();
        state.receiver.replace(Some(rx));
        let wake = state.wake.get().unwrap().handle();
        std::thread::spawn(move || {
            let snapshot = objc2::rc::autoreleasepool(|_| read_window_snapshot(identities));
            let _ = tx.send(snapshot);
            wake.signal();
        });
        self.render();
    }

    pub(super) fn install_window_snapshot(&self, snapshot: WindowSnapshot) {
        let state = self.ivars();
        let WindowSnapshot {
            mut windows,
            identities,
            mut server_ids,
        } = snapshot;
        windows.retain(|window| !state.closed_windows.borrow().contains(&window.id));
        server_ids.retain(|id, _| !state.closed_windows.borrow().contains(id));
        state.closed_windows.borrow_mut().clear();
        let retained: HashSet<_> = state
            .windows
            .borrow()
            .iter()
            .chain(&windows)
            .map(|w| w.id)
            .collect();
        let mut ids = state.window_server_ids.borrow_mut();
        ids.retain(|id, _| retained.contains(id));
        ids.extend(server_ids);
        drop(ids);
        self.install_identities(identities);
        let snapshot_ready = state
            .switch_selection
            .borrow()
            .as_ref()
            .is_some_and(|selection| selection.selected().is_some());
        if state.mode.get() == Some(PanelMode::Switch) && snapshot_ready {
            state.deferred_windows.replace(Some(windows));
            self.select_alias();
            self.render();
            self.commit_switch_if_ready();
        } else {
            let selected_id = self.selected_result();
            self.install_windows(windows);
            self.filter_preserving(selected_id);
            self.prepare_switch_selection();
            self.commit_switch_if_ready();
        }
        self.schedule_cache_warmup();
        self.check_window_liveness();
    }

    pub(super) fn install_identities(&self, identities: HashMap<i32, AppIdentity>) {
        let state = self.ivars();
        let previous = state.identities.borrow();
        state.icons.borrow_mut().retain(|pid, _| {
            identities
                .get(pid)
                .is_some_and(|app| previous.get(pid).is_some_and(|old| old.id == app.id))
        });
        drop(previous);
        state.identities.replace(identities);
    }

    pub(super) fn install_windows(&self, windows: Vec<WindowInfo>) {
        crate::macos::platform::recency_trace::record("inventory", || {
            format!(
                "windows={:?} apps={:?} recent={:?}",
                windows.iter().map(|w| (w.id, w.pid)).collect::<Vec<_>>(),
                self.ivars()
                    .identities
                    .borrow()
                    .iter()
                    .map(|(pid, app)| (*pid, &app.id))
                    .collect::<Vec<_>>(),
                self.ivars().recency.borrow(),
            )
        });
        self.update_aliases(&windows);
        self.ivars().windows.replace(windows);
    }

    pub(super) fn update_aliases(&self, windows: &[WindowInfo]) {
        let mut aliases = self.ivars().automatic_aliases.borrow_mut();
        if aliases.ensure_windows(windows, &self.ivars().identities.borrow())
            && self.ivars().aliases_writable.get()
        {
            preference_store::save_aliases(&aliases);
        }
        let effective = aliases.with_rules(
            windows,
            &self.ivars().identities.borrow(),
            &self.ivars().config.borrow().alias_rules,
        );
        self.ivars().aliases.replace(effective);
    }

    pub(super) fn ensure_app_catalog(&self) {
        let state = self.ivars();
        if state.demo.get()
            || self.scoped_search()
            || state.mode.get() != Some(PanelMode::Search)
            || state.current_app_only.get()
            || state.query.borrow().trim().is_empty()
            || state.catalog_receiver.borrow().is_some()
            || state
                .catalog_checked
                .get()
                .is_some_and(|at| at.elapsed() < APP_CATALOG_TTL)
        {
            return;
        }
        let (tx, rx) = mpsc::channel();
        state.catalog_receiver.replace(Some(rx));
        let wake = state.wake.get().unwrap().handle();
        std::thread::spawn(move || {
            let _ = tx.send(installed_apps::discover());
            wake.signal();
        });
    }

    pub(super) fn poll_app_catalog(&self) {
        let state = self.ivars();
        let result = state
            .catalog_receiver
            .borrow()
            .as_ref()
            .map(|rx| rx.try_recv());
        match result {
            Some(Ok(apps)) => {
                let selected = self.selected_result();
                state.catalog_receiver.replace(None);
                state.catalog_checked.set(Some(Instant::now()));
                state.installed_apps.replace(apps);
                if state.mode.get() == Some(PanelMode::Search) {
                    self.filter_preserving(selected);
                }
            }
            Some(Err(TryRecvError::Disconnected)) => {
                state.catalog_receiver.replace(None);
            }
            _ => {}
        }
    }

    pub(super) fn schedule_cache_warmup(&self) {
        let state = self.ivars();
        if state.demo.get()
            || state.cache_warmup_timer.borrow().is_some()
            || state.windows.borrow().is_empty()
            || state.panels.borrow().is_empty()
        {
            return;
        }
        // One icon or row per turn keeps preparation out of shortcut handling.
        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                0.01,
                self,
                sel!(warmPanelCache:),
                None,
                true,
            )
        };
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
        state.cache_warmup_timer.replace(Some(timer));
    }

    pub(super) fn warm_cache_step(&self) -> bool {
        let state = self.ivars();
        let missing_icon = state
            .windows
            .borrow()
            .iter()
            .find(|window| !state.icons.borrow().contains_key(&window.pid))
            .map(|window| window.pid);
        if let Some(pid) = missing_icon {
            let image = self.icon(pid);
            for ui in self.panels() {
                for row in ui.rows.borrow().iter() {
                    if matches!(&row.content, Some(RowContent::Window(window, _)) if window.pid == pid)
                    {
                        row.icon.setImage(image.as_deref());
                    }
                }
            }
            return true;
        }
        if state.mode.get().is_none() {
            let density = state.config.borrow().display_density;
            for ui in self.panels() {
                let height = ui
                    .panel
                    .screen()
                    .map_or(HEIGHT, |screen| screen.visibleFrame().size.height);
                let count = state
                    .windows
                    .borrow()
                    .len()
                    .min((height / row_height(density)).ceil() as usize);
                let mut rows = ui.rows.borrow_mut();
                if rows.len() < count {
                    let position = rows.len();
                    rows.push(self.create_row(position, density));
                    return true;
                }
            }
        }
        false
    }

    pub(super) fn cached_icon(&self, pid: i32) -> Option<Retained<NSImage>> {
        self.ivars()
            .icons
            .borrow()
            .get(&pid)
            .cloned()
            .unwrap_or_else(|| self.placeholder_icon())
    }

    pub(super) fn placeholder_icon(&self) -> Option<Retained<NSImage>> {
        self.ivars()
            .placeholder_icon
            .get_or_init(|| {
                NSImage::imageWithSystemSymbolName_accessibilityDescription(
                    ns_string!("macwindow"),
                    Some(&NSString::from_str(tr!("窗口", "Window"))),
                )
            })
            .clone()
    }

    pub(super) fn icon(&self, pid: i32) -> Option<Retained<NSImage>> {
        self.ivars()
            .icons
            .borrow_mut()
            .entry(pid)
            .or_insert_with(|| {
                NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
                    .and_then(|app| app.icon())
                    .or_else(|| self.placeholder_icon())
            })
            .clone()
    }

    pub(super) fn activate_app(&self, app: &NSRunningApplication) -> bool {
        let current = NSRunningApplication::currentApplication();
        NSApplication::sharedApplication(self.mtm()).yieldActivationToApplication(app);
        app.activateFromApplication_options(&current, NSApplicationActivationOptions::empty())
    }

    pub(super) fn application_icon(&self, app: &ApplicationTarget) -> Retained<NSImage> {
        self.ivars()
            .application_icons
            .borrow_mut()
            .entry(app.path.clone())
            .or_insert_with(|| {
                let source =
                    NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(&app.path));
                let image = NSImage::initWithSize(NSImage::alloc(), NSSize::new(24.0, 24.0));
                #[allow(deprecated)]
                {
                    image.lockFocus();
                    source.drawInRect(rect(0.0, 0.0, 24.0, 24.0));
                    image.unlockFocus();
                }
                image
            })
            .clone()
    }
}

pub(super) fn read_window_snapshot(cached: HashMap<i32, AppIdentity>) -> WindowSnapshot {
    let mut identities = HashMap::new();
    // runningApplications is thread-safe. Keep bundle metadata and AX reads
    // together in the worker; only plain Rust data crosses back to AppKit.
    let apps = NSWorkspace::sharedWorkspace()
        .runningApplications()
        .iter()
        .filter(|app| {
            app.processIdentifier() != std::process::id() as i32
                && !app.isTerminated()
                && app.activationPolicy() == NSApplicationActivationPolicy::Regular
        })
        .map(|app| {
            let pid = app.processIdentifier();
            let id = app
                .bundleIdentifier()
                .map(|id| id.to_string())
                .or_else(|| app.bundleURL()?.path().map(|path| path.to_string()));
            let identity = cached
                .get(&pid)
                .filter(|app| Some(app.id.as_str()) == id.as_deref())
                .cloned()
                .or_else(|| app_identity(&app));
            if let Some(identity) = identity {
                identities.insert(pid, identity);
            }
            (
                pid,
                app.localizedName()
                    .map(|name| name.to_string())
                    .unwrap_or_else(|| "Application".into()),
            )
        })
        .collect::<Vec<_>>();
    let (windows, server_ids) = accessibility::list_windows(&apps);
    WindowSnapshot {
        windows,
        identities,
        server_ids,
    }
}

pub(super) fn app_identity(app: &NSRunningApplication) -> Option<AppIdentity> {
    let url = app.bundleURL()?;
    let id = app
        .bundleIdentifier()
        .map(|id| id.to_string())
        .or_else(|| url.path().map(|path| path.to_string()))?;
    let info = NSBundle::bundleWithURL(&url).and_then(|bundle| bundle.infoDictionary());
    let metadata_name = ["CFBundleDisplayName", "CFBundleName", "CFBundleExecutable"]
        .iter()
        .filter_map(|key| {
            info.as_ref()?
                .objectForKey(&NSString::from_str(key))?
                .downcast::<NSString>()
                .ok()
                .map(|name| name.to_string())
        })
        .find(|name| name.chars().any(|ch| ch.is_ascii_alphabetic()));
    let english_name = metadata_name
        .or_else(|| {
            url.lastPathComponent()
                .map(|name| name.to_string().trim_end_matches(".app").to_owned())
        })
        .unwrap_or_default();
    Some(AppIdentity { id, english_name })
}

pub(super) fn demo_windows() -> Vec<WindowInfo> {
    [
        ("Safari", "Rust documentation — ownership and borrowing"),
        ("Visual Studio Code", "main.rs — Winlane"),
        ("Terminal", "cargo test — winlane"),
        (
            "Obsidian",
            tr!("项目笔记 · 窗口切换器", "Project notes · Window switcher"),
        ),
        ("Safari", "AppKit — Apple Developer"),
        ("Finder", "Downloads"),
        ("Notes", tr!("本周计划", "This week's plan")),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (app, title))| WindowInfo {
        id: i as u64,
        pid: if app == "Safari" { -1 } else { -(i as i32 + 2) },
        app: app.into(),
        title: title.into(),
        minimized: i == 5,
    })
    .collect()
}
