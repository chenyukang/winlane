#![allow(dead_code)]

#[cfg(target_os = "macos")]
#[path = "../src/snippet_paste.rs"]
mod snippet_paste;
#[cfg(target_os = "macos")]
mod snippet_ui {
    include!("../src/snippet_ui.rs");
    include!("support/snippet_ui.rs");
}

#[cfg(target_os = "macos")]
#[path = "../src/project_open.rs"]
mod project_open;

#[cfg(target_os = "macos")]
#[path = "../src/quicklink_input.rs"]
mod quicklink_input;

#[cfg(target_os = "macos")]
mod quicklink_ui {
    include!("../src/quicklink_ui.rs");
    include!("support/quicklink_ui.rs");
}

#[cfg(target_os = "macos")]
mod updater {
    include!("../src/updater.rs");
    include!("support/updater.rs");
}

#[cfg(target_os = "macos")]
#[path = "../src/main_wake.rs"]
mod main_wake;

#[cfg(target_os = "macos")]
mod system_commands {
    include!("../src/system_commands.rs");
    include!("support/system_commands.rs");
}

#[cfg(target_os = "macos")]
mod menu_bar {
    include!("../src/menu_bar.rs");

    pub fn verify_reveal_positions() {
        for (x, y, width, height) in [
            (0.0, 0.0, 1728.0, 1117.0),
            (-2560.0, 0.0, 2560.0, 1440.0),
            (0.0, -1440.0, 2560.0, 1440.0),
        ] {
            let point = reveal_position(NSRect::new(
                NSPoint::new(x, y),
                objc2_foundation::NSSize::new(width, height),
            ));
            assert_eq!(point.x, x + width * 0.75);
            assert_eq!(point.y, y + 1.0);
            assert!(point.x > x && point.x < x + width);
            assert!(point.y >= y && point.y < y + height);
        }
    }
}

#[cfg(target_os = "macos")]
#[path = "../src/focus_observer.rs"]
mod focus_observer;
#[cfg(target_os = "macos")]
#[path = "../src/input_source.rs"]
mod input_source;

#[cfg(target_os = "macos")]
mod accessibility {
    include!("../src/accessibility.rs");

    pub fn inspect_published_window_changes(pid: i32) {
        let mut inventory = Inventory::read();
        let application = Element::application(pid).unwrap();
        let windows = all_windows(&application, pid, &inventory);
        let expected: HashSet<_> = windows.iter().filter_map(Element::server_id).collect();
        assert!(
            expected.len() >= 3,
            "requires three independent live windows"
        );
        remote_scans().lock().unwrap().remove(&pid);

        // Substitute successive published snapshots without changing the user's
        // active window or Space. Remote lookups still use the live AX objects.
        for round in 0..12 {
            let published = &windows[round % windows.len()];
            let found = complete_windows(vec![Element(published.0.clone())], pid, &inventory);
            let found: HashSet<_> = found.iter().filter_map(Element::server_id).collect();
            println!(
                "published transition={} current={:?} windows={} ids={found:?}",
                round + 1,
                published.server_id(),
                found.len()
            );
            assert_eq!(
                found, expected,
                "a change of published window lost another window"
            );
        }

        let found = complete_windows(Vec::new(), pid, &inventory);
        let found: HashSet<_> = found.iter().filter_map(Element::server_id).collect();
        assert_eq!(
            found, expected,
            "an empty published list must recover live windows"
        );

        let closed = windows.last().unwrap().server_id().unwrap();
        inventory.normal.get_mut(&pid).unwrap().remove(&closed);
        let found = complete_windows(vec![Element(windows[0].0.clone())], pid, &inventory);
        let found: HashSet<_> = found.iter().filter_map(Element::server_id).collect();
        assert!(
            !found.contains(&closed),
            "closed windows must leave the cache"
        );
        assert_eq!(found.len(), expected.len() - 1);
        inventory.normal.get_mut(&pid).unwrap().insert(closed);
        let found = complete_windows(vec![Element(windows[0].0.clone())], pid, &inventory);
        let found: HashSet<_> = found.iter().filter_map(Element::server_id).collect();
        assert_eq!(
            found, expected,
            "a new inventory entry must restart discovery"
        );
        println!("Empty published list, closed window removal, and inventory addition passed.");
        remote_scans().lock().unwrap().remove(&pid);
    }
}
#[cfg(target_os = "macos")]
mod alias_rules {
    include!("../src/alias_rules.rs");
    include!("support/alias_rules.rs");
}
#[cfg(target_os = "macos")]
mod app_shortcuts {
    include!("../src/app_shortcuts.rs");
    include!("support/app_shortcuts.rs");
}
#[cfg(target_os = "macos")]
mod installed_apps {
    include!("../src/installed_apps.rs");
    include!("support/installed_apps.rs");
}
#[cfg(target_os = "macos")]
mod settings {
    include!("../src/settings.rs");
    include!("support/localization.rs");
    include!("support/settings_escape.rs");
}
#[cfg(target_os = "macos")]
#[path = "../src/shortcut_tap.rs"]
mod shortcut_tap;
#[cfg(target_os = "macos")]
#[allow(unused_imports)]
#[path = "../src/window_server.rs"]
mod window_server;

#[cfg(target_os = "macos")]
mod app {
    include!("../src/app.rs");
    include!("support/launch_search.rs");
    include!("support/commands.rs");
    include!("support/snippet_search.rs");
    include!("support/quicklink_search.rs");
    include!("support/projects.rs");
    include!("support/clipboard.rs");
    include!("support/responsiveness.rs");

    pub fn inspect_window_discovery(bundle: &str) {
        assert!(
            accessibility::is_trusted(),
            "read-only scan requires existing accessibility access"
        );
        let apps: Vec<_> = NSWorkspace::sharedWorkspace()
            .runningApplications()
            .iter()
            .filter(|app| {
                app.bundleIdentifier()
                    .is_some_and(|id| id.to_string() == bundle)
            })
            .map(|app| {
                (
                    app.processIdentifier(),
                    app.localizedName().unwrap().to_string(),
                )
            })
            .collect();
        assert!(!apps.is_empty(), "requested application is not running");
        if std::env::var_os("WINLANE_SCAN_TRANSITIONS").is_some() {
            for (pid, _) in &apps {
                accessibility::inspect_published_window_changes(*pid);
            }
            return;
        }
        for round in 0..12 {
            let start = Instant::now();
            let windows = accessibility::list_windows(&apps);
            println!(
                "scan round={} windows={} elapsed_ms={:.1} ids={:?}",
                round + 1,
                windows.len(),
                start.elapsed().as_secs_f64() * 1000.0,
                windows.iter().map(|window| window.id).collect::<Vec<_>>()
            );
            if let Ok(minimum) = std::env::var("WINLANE_EXPECT_WINDOWS") {
                let minimum: usize = minimum
                    .parse()
                    .expect("expected window count must be numeric");
                assert!(
                    windows.len() >= minimum,
                    "scan omitted independently verified windows"
                );
            }
        }
    }

    pub fn benchmark_hidden_panels() {
        use std::time::Instant;
        let start = Instant::now();
        let mtm = MainThreadMarker::new().unwrap();
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Prohibited);
        let delegate = Delegate::new(mtm);
        delegate.ivars().demo.set(true);
        delegate.ivars().mode.set(Some(PanelMode::Search));
        objc2::rc::autoreleasepool(|_| delegate.sync_displays());
        let panel_init_ms = start.elapsed().as_secs_f64() * 1000.0;
        let apps: Vec<_> = NSWorkspace::sharedWorkspace()
            .runningApplications()
            .iter()
            .filter(|app| app.activationPolicy() == NSApplicationActivationPolicy::Regular)
            .map(|app| {
                (
                    app.processIdentifier(),
                    app.localizedName().unwrap().to_string(),
                )
            })
            .collect();
        let count = std::env::var("WINLANE_BENCH_WINDOWS")
            .ok()
            .and_then(|count| count.parse().ok())
            .unwrap_or(24);
        let windows = (0..count)
            .map(|index| {
                let (pid, name) = &apps[index % apps.len()];
                WindowInfo {
                    id: index as u64,
                    pid: *pid,
                    app: name.clone(),
                    title: format!("project-{index} — Rust documentation"),
                    minimized: false,
                }
            })
            .collect();
        delegate.ivars().windows.replace(windows);
        let start = Instant::now();
        objc2::rc::autoreleasepool(|_| delegate.filter());
        let first_list_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        while delegate.warm_cache_step() {}
        let deferred_icons_ms = start.elapsed().as_secs_f64() * 1000.0;
        let before = memory_sample();
        let mut selection = Vec::new();
        for _ in 0..120 {
            let start = Instant::now();
            objc2::rc::autoreleasepool(|_| delegate.move_selection(1));
            selection.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        let mut search = Vec::new();
        for query in ["project", "rust", "project-1", "no-match", ""]
            .into_iter()
            .cycle()
            .take(50)
        {
            let start = Instant::now();
            objc2::rc::autoreleasepool(|_| {
                delegate.ivars().query.replace(query.into());
                delegate.filter();
            });
            search.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        selection.sort_by(f64::total_cmp);
        search.sort_by(f64::total_cmp);
        let mut prepare_search = Vec::new();
        for session in 0..30 {
            delegate.end_session();
            let start = Instant::now();
            objc2::rc::autoreleasepool(|_| delegate.prepare_panel(PanelMode::Search, session, 0));
            prepare_search.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        prepare_search.sort_by(f64::total_cmp);
        let after = memory_sample();
        assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));
        println!(
            "{}",
            serde_json::json!({
                "windows":count, "displays":delegate.panels().len(),
                "panel_init_ms":panel_init_ms, "first_list_ms":first_list_ms,
                "deferred_icons_ms":deferred_icons_ms,
                "prepare_search_median_ms":prepare_search[prepare_search.len()/2],
                "prepare_search_p95_ms":prepare_search[prepare_search.len()*95/100],
                "selection_median_ms":selection[selection.len()/2],
                "selection_p95_ms":selection[selection.len()*95/100],
                "search_median_ms":search[search.len()/2],
                "search_p95_ms":search[search.len()*95/100],
                "memory_before":before,"memory_after":after
            })
        );
        delegate.end_session();
    }

    fn memory_sample() -> serde_json::Value {
        let pid = std::process::id().to_string();
        let rss = std::process::Command::new("/bin/ps")
            .args(["-p", &pid, "-o", "rss="])
            .output()
            .unwrap();
        let map = std::process::Command::new("/usr/bin/vmmap")
            .args(["-summary", &pid])
            .output()
            .unwrap();
        let map = String::from_utf8_lossy(&map.stdout);
        serde_json::json!({
            "rss_kib":String::from_utf8_lossy(&rss.stdout).trim().parse::<u64>().unwrap(),
            "footprint":map.lines().find(|line| line.starts_with("Physical footprint:")).unwrap_or("unavailable")
        })
    }

    pub fn verify_hidden_panels() {
        let mtm = MainThreadMarker::new().expect("native checks must run on the main thread");
        let screens = NSScreen::screens(mtm);
        if screens.is_empty() {
            println!("Native panel checks skipped: no graphical display.");
            return;
        }
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Prohibited);
        verify_main_wake(mtm);
        verify_switch_alias_prefix(mtm);
        verify_distinct_window_aliases(mtm);
        verify_alias_does_not_block_title_search(mtm);
        let delegate = Delegate::new(mtm);
        crate::settings::verify_localized_settings(&delegate, mtm);
        verify_autosave(mtm);
        for language in ["en", "zh"] {
            if let Some(source) = Source::for_language(language, mtm) {
                let id = source
                    .id()
                    .expect("language source needs a stable identifier");
                assert_eq!(
                    Source::by_id(&id, mtm).and_then(|source| source.id()),
                    Some(id)
                );
            }
        }
        assert!(Source::by_id("example.test.missing-input-source", mtm).is_none());
        crate::app_shortcuts::verify_hidden_settings(&delegate, mtm);
        crate::installed_apps::verify_catalog();
        verify_catalog_refresh(mtm);
        verify_launch_search(mtm);
        verify_command_search(mtm);
        verify_snippet_search(mtm);
        verify_quicklink_search(mtm);
        verify_projects_search(mtm);
        verify_clipboard_search(mtm);
        verify_clipboard_images(mtm);
        crate::snippet_ui::verify_editor(&delegate, mtm);
        crate::system_commands::verify_prepared_commands(mtm);
        crate::menu_bar::verify_reveal_positions();
        verify_typo_search(mtm);
        verify_shortcut_recency(mtm);
        verify_external_focus_history(mtm);
        verify_switch_delay(mtm);
        verify_responsive_panels(mtm);
        verify_async_focus_order(mtm);
        verify_async_window_snapshot(mtm);
        verify_project_rule_search(mtm);
        verify_adaptive_panels(mtm);
        verify_display_density(mtm);
        verify_usage_hint_visibility(mtm);
        verify_app_name_search(mtm);
        verify_editor_window_titles(mtm);
        delegate.ivars().demo.set(true);
        delegate.ivars().windows.replace(demo_windows());
        delegate.ivars().mode.set(Some(PanelMode::Search));
        delegate.sync_displays();
        delegate.filter();
        let panels = delegate.panels();
        for policy in [
            winlane::input_method::InputMethod::English,
            winlane::input_method::InputMethod::Chinese,
        ] {
            delegate.ivars().config.borrow_mut().input_method = policy;
            delegate.prepare_search_input();
            delegate.start_input_gate_timer();
            let pending = delegate.ivars().input_start_timer.borrow().clone();
            delegate.focus_search();
            assert!(
                !delegate.ivars().input_session.borrow().focused,
                "hidden panels must never change or remember input sources"
            );
            assert!(delegate.ivars().input_gate.borrow().target().is_none());
            assert!(delegate.ivars().input_start_timer.borrow().is_none());
            assert!(pending.as_ref().is_none_or(|timer| !timer.isValid()));
            delegate.prepare_search_input();
            delegate.start_input_gate_timer();
            let next = delegate.ivars().input_start_timer.borrow().clone();
            if let Some(stale) = pending {
                delegate.finish_input_start(sel!(finishSearchInputStart:), &stale);
                assert!(next.as_ref().is_none_or(|timer| timer.isValid()));
            }
            delegate.finish_search_input();
            assert!(delegate.ivars().input_gate.borrow().target().is_none());
            assert!(delegate.ivars().input_start_timer.borrow().is_none());
            assert!(next.as_ref().is_none_or(|timer| !timer.isValid()));
        }
        delegate.ivars().config.borrow_mut().input_method =
            winlane::input_method::InputMethod::Current;
        let mut distinct_frames = Vec::new();
        for screen in screens.iter() {
            if !distinct_frames.contains(&screen.frame()) {
                distinct_frames.push(screen.frame());
            }
        }
        assert_eq!(panels.len(), distinct_frames.len());
        assert!(panels.iter().all(|ui| !ui.panel.isVisible()));
        for ui in &panels {
            let frame = ui.panel.frame();
            assert!(screens.iter().any(|screen| {
                let visible = screen.visibleFrame();
                frame.origin.x >= visible.origin.x
                    && frame.origin.y >= visible.origin.y
                    && frame.origin.x + frame.size.width
                        <= visible.origin.x + visible.size.width + 1.0
                    && frame.origin.y + frame.size.height
                        <= visible.origin.y + visible.size.height + 1.0
            }));
            assert!(
                ui.panel
                    .collectionBehavior()
                    .contains(NSWindowCollectionBehavior::CanJoinAllSpaces)
            );
            assert!(
                !ui.panel
                    .collectionBehavior()
                    .contains(NSWindowCollectionBehavior::MoveToActiveSpace)
            );
        }
        // Exercise typing from a secondary panel through the production delegate.
        let input = &panels.last().unwrap().input;
        input.setStringValue(ns_string!("Safari"));
        let notification = unsafe {
            NSNotification::notificationWithName_object(
                ns_string!("NSControlTextDidChangeNotification"),
                Some(input),
            )
        };
        unsafe {
            let _: () = msg_send![&*delegate, controlTextDidChange: &*notification];
        }
        assert_eq!(delegate.ivars().matches.borrow().len(), 2);
        for ui in &panels {
            assert_eq!(ui.input.stringValue().to_string(), "Safari");
            assert_eq!(ui.list.subviews().len(), 2);
            assert!(!ui.input.isHidden());
        }
        let rows_before: Vec<_> = panels.iter().map(|ui| ui.list.subviews()).collect();
        delegate.move_selection(1);
        for (ui, rows) in panels.iter().zip(&rows_before) {
            assert_eq!(
                ui.list.subviews(),
                *rows,
                "moving selection must reuse row views"
            );
            let selection: Vec<_> = ui
                .list
                .subviews()
                .iter()
                .map(|row| row.isAccessibilitySelected())
                .collect();
            assert_eq!(selection, [false, true]);
        }
        delegate.ivars().query.replace("no matching window".into());
        delegate.filter();
        delegate.ivars().query.replace("Safari".into());
        delegate.filter();
        for (ui, rows) in panels.iter().zip(&rows_before) {
            assert_eq!(
                ui.list.subviews(),
                *rows,
                "filtering should reuse detached rows"
            );
        }
        let matched = delegate.ivars().matches.borrow()[0];
        delegate.ivars().windows.borrow_mut()[matched].title = "Updated title".into();
        delegate.render();
        for ui in &panels {
            let rows = ui.rows.borrow();
            assert_eq!(rows[0].title.stringValue().to_string(), "Updated title");
            assert_eq!(rows[0].icon.image(), rows[1].icon.image());
        }
        let scope = delegate.menu_item("Current app only", sel!(toggleScope:), "");
        delegate.ivars().query.replace(String::new());
        unsafe {
            let _: () = msg_send![&*delegate, toggleScope: &*scope];
            let _: bool = msg_send![&*delegate, validateMenuItem: &*scope];
        }
        assert!(delegate.ivars().current_app_only.get());
        assert_eq!(scope.state(), NSControlStateValueOn);
        assert!(panels.iter().all(|ui| ui.list.subviews().len() == 2));
        unsafe {
            let _: () = msg_send![&*delegate, toggleScope: &*scope];
            let _: bool = msg_send![&*delegate, validateMenuItem: &*scope];
        }
        assert!(!delegate.ivars().current_app_only.get());
        assert_eq!(scope.state(), NSControlStateValueOff);
        delegate.ivars().mode.set(Some(PanelMode::Switch));
        delegate.render();
        assert!(
            panels
                .iter()
                .all(|ui| ui.input.isHidden() && !ui.mode_label.isHidden())
        );
        delegate.ivars().alias_input.borrow_mut().push('w');
        delegate.render();
        assert!(
            panels
                .iter()
                .all(|ui| ui.mode_label.stringValue().to_string().contains("Alias  w"))
        );
        delegate.display_search(0);
        assert!(
            panels
                .iter()
                .all(|ui| !ui.input.isHidden() && ui.mode_label.isHidden())
        );
        for mode in [PanelMode::Search, PanelMode::Switch] {
            delegate.ivars().mode.set(Some(mode));
            for percent in [0, 50, 100] {
                delegate.ivars().config.borrow_mut().background_opacity = percent;
                delegate.render();
                for ui in &panels {
                    let root = ui.panel.contentView().unwrap();
                    assert!(!ui.panel.isOpaque());
                    assert_eq!(ui.panel.alphaValue(), 1.0);
                    assert_eq!(root.alphaValue(), 1.0);
                    assert_eq!(ui.backdrop.alphaValue(), f64::from(percent) / 100.0);
                    assert_eq!(ui.backdrop.frame(), root.bounds());
                    assert_eq!(
                        ui.backdrop.subviews().objectAtIndex(0).frame(),
                        ui.backdrop.bounds()
                    );
                    assert_eq!(ui.input.alphaValue(), 1.0);
                    assert_eq!(unsafe { ui.input.superview() }, Some(root.clone()));
                    for row in ui.rows.borrow().iter() {
                        assert_eq!(row.button.alphaValue(), 1.0);
                        assert_eq!(row.title.alphaValue(), 1.0);
                        assert_eq!(row.icon.alphaValue(), 1.0);
                    }
                    assert!(!ui.panel.isVisible());
                }
            }
        }
        delegate.ivars().mode.set(Some(PanelMode::Search));
        delegate.render();
        let ids: Vec<_> = panels.iter().map(|ui| ui.panel.windowNumber()).collect();
        delegate.sync_displays();
        assert_eq!(
            delegate
                .panels()
                .iter()
                .map(|ui| ui.panel.windowNumber())
                .collect::<Vec<_>>(),
            ids
        );
        if let Ok(directory) = std::env::var("WINLANE_PREVIEW_DIR") {
            export_hidden_previews(&delegate, &directory);
        }
        delegate.end_session();
        assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));
        let retained_rows = panels[0].list.subviews();
        delegate.ivars().query.replace("nothing matches".into());
        delegate.filter();
        assert_eq!(
            panels[0].list.subviews(),
            retained_rows,
            "closed panels must not redraw"
        );
        println!(
            "Native panel checks passed on {} display(s): placement, shared query/scope/mode/alias, reuse; no panels shown or shortcuts registered.",
            panels.len()
        );
    }

    fn verify_editor_window_titles(mtm: MainThreadMarker) {
        let delegate = Delegate::new(mtm);
        let state = delegate.ivars();
        state.demo.set(true);
        state.mode.set(Some(PanelMode::Search));
        delegate.sync_displays();
        let cases = [
            (
                "com.microsoft.VSCode",
                "snapshot2.rs — ckb",
                "ckb: snapshot2.rs",
            ),
            (
                "com.microsoft.VSCode",
                "README.md (added in abc123) (README.md ((deleted)) ↔ README.md (Working Tree)) — project",
                "project: README.md (added in abc123) (README.md ((deleted)) ↔ README.md (Working Tree))",
            ),
            (
                "com.microsoft.VSCode",
                "● main.rs — my - project — Visual Studio Code",
                "my - project: ● main.rs",
            ),
            (
                "com.microsoft.VSCodeInsiders",
                "main.rs — project [SSH: dev] — Visual Studio Code - Insiders",
                "project [SSH: dev]: main.rs",
            ),
            (
                "com.microsoft.VSCode",
                "main.rs - project - Visual Studio Code",
                "project: main.rs",
            ),
            (
                "com.microsoft.VSCodeInsiders",
                "main.rs - project - Visual Studio Code - Insiders",
                "project: main.rs",
            ),
            (
                "com.microsoft.VSCode",
                "Welcome — Visual Studio Code",
                "Welcome — Visual Studio Code",
            ),
            ("com.microsoft.VSCode", "project", "project"),
            ("com.microsoft.VSCode", "main.rs — ", "main.rs — "),
            (
                "com.example.other",
                "snapshot2.rs — ckb",
                "snapshot2.rs — ckb",
            ),
        ];
        for (bundle, original, expected) in cases {
            state.identities.borrow_mut().insert(
                -100,
                AppIdentity {
                    id: bundle.into(),
                    english_name: "Code".into(),
                },
            );
            let window = WindowInfo {
                id: 100,
                pid: -100,
                app: "Code".into(),
                title: original.into(),
                minimized: false,
            };
            state.windows.replace(vec![window.clone()]);
            for mode in [PanelMode::Search, PanelMode::Switch] {
                state.mode.set(Some(mode));
                state.query.borrow_mut().clear();
                delegate.filter();
                assert_eq!(delegate.selected_window().unwrap(), window);
                for ui in delegate.panels() {
                    assert_eq!(
                        ui.rows.borrow()[0].title.stringValue().to_string(),
                        expected
                    );
                    assert!(!ui.panel.isVisible());
                }
            }
        }
        state.identities.borrow_mut().get_mut(&-100).unwrap().id = "com.microsoft.VSCode".into();
        state.windows.borrow_mut()[0].minimized = true;
        state.mode.set(Some(PanelMode::Search));
        state.query.replace("snapshot2.rs ckb".into());
        delegate.filter();
        assert_eq!(
            delegate.match_count(),
            1,
            "search must still use the original title"
        );
        for ui in delegate.panels() {
            assert_eq!(
                ui.rows.borrow()[0].title.stringValue().to_string(),
                "ckb: snapshot2.rs · 已最小化"
            );
        }
        println!("VS Code project-first titles passed in search and switch panels.");
    }

    fn verify_autosave(mtm: MainThreadMarker) {
        let previous_locale = winlane::i18n::locale();
        winlane::i18n::set_locale(winlane::i18n::Locale::English);
        let domain = NSString::from_str(&format!(
            "com.example.winlane-autosave-test-{}",
            std::process::id()
        ));
        let store =
            NSUserDefaults::initWithSuiteName(NSUserDefaults::alloc(), Some(&domain)).unwrap();
        store.removePersistentDomainForName(&domain);
        let delegate = Delegate::new(mtm);
        delegate.ivars().config_store.set(store.clone()).unwrap();
        let settings = delegate.ensure_settings_window();
        settings.fill(&Config::default());
        let saved = || {
            let text = store
                .stringForKey(ns_string!("WindowlanePreferencesV1"))
                .unwrap();
            let saved = Config::from_json(&text.to_string()).unwrap();
            assert_eq!(saved, *delegate.ivars().config.borrow());
            assert!(
                delegate.ivars().shortcut_tap.borrow().is_none(),
                "ordinary settings must not register a keyboard tap"
            );
            saved
        };
        crate::settings::verify_autosave_controls(&settings, saved);
        crate::settings::verify_sidebar_actions(&settings, saved);
        unsafe {
            let _: () = msg_send![&*delegate, resetSettings: None::<&AnyObject>];
        }
        assert_eq!(
            saved(),
            Config::default(),
            "Restore Defaults must persist immediately"
        );
        let seed = Config {
            app_shortcuts: vec![winlane::config::AppShortcut {
                application: ApplicationTarget {
                    bundle_id: "com.example.browser".into(),
                    path: "/Applications/Example Browser.app".into(),
                    name: "Browser".into(),
                },
                shortcut: winlane::config::Shortcut {
                    command: true,
                    control: false,
                    option: false,
                    shift: false,
                    key: "Digit1".into(),
                },
            }],
            ..Config::default()
        };
        delegate.ivars().config.replace(seed.clone());
        crate::settings::save(&seed, &store).unwrap();
        let shortcuts = delegate.ensure_app_shortcuts_window();
        shortcuts.fill(&seed.app_shortcuts, &delegate, mtm);
        crate::app_shortcuts::verify_autosave_target(&shortcuts, &delegate, mtm, saved);
        assert!(!settings.window.isVisible() && !shortcuts.window.isVisible());
        let rules = delegate.ensure_alias_rules_window();
        crate::settings::verify_escape_close(&rules.window);
        crate::alias_rules::verify_rules_editor(&rules, &delegate, mtm, saved);
        assert_eq!(
            settings.candidate().unwrap().alias_rules,
            saved().alias_rules
        );
        crate::settings::verify_escape_close(&settings.window);
        crate::settings::verify_escape_close(&shortcuts.window);
        crate::settings::verify_escape_autosave(&settings, saved);
        println!(
            "Settings Escape checks passed: all three windows close, active edits save, IME composition and modified Escape do not close windows."
        );
        store.removePersistentDomainForName(&domain);
        winlane::i18n::set_locale(previous_locale);
        println!(
            "Autosave checks passed: native actions persisted to an isolated preferences domain; invalid values and conflicts preserved prior settings; no keyboard taps installed."
        );
    }

    fn verify_app_name_search(mtm: MainThreadMarker) {
        let delegate = Delegate::new(mtm);
        let state = delegate.ivars();
        state.demo.set(true);
        state.mode.set(Some(PanelMode::Search));
        state.windows.replace(vec![
            WindowInfo {
                id: 1,
                pid: -1,
                app: "Code".into(),
                title: "localization.rs (Working Tree) (localization.rs) — windowlane".into(),
                minimized: false,
            },
            WindowInfo {
                id: 2,
                pid: -2,
                app: "Notion".into(),
                title: "CKB Dev Log".into(),
                minimized: false,
            },
        ]);
        state.recency.replace(vec![1, 2]);
        delegate.sync_displays();
        delegate.filter();
        assert_eq!(delegate.selected_window().unwrap().id, 1);
        for query in ["noti", "notion"] {
            state.query.replace(query.into());
            delegate.filter();
            assert_eq!(delegate.selected_window().unwrap().id, 2);
            assert_eq!(delegate.match_count(), 2);
            for ui in delegate.panels() {
                let rows = ui.rows.borrow();
                assert_eq!(rows[0].app.stringValue().to_string(), "Notion");
                assert!(rows[0].button.isAccessibilitySelected());
                assert_eq!(rows[1].app.stringValue().to_string(), "Code");
                assert!(!ui.panel.isVisible());
            }
        }
        println!(
            "Partial and exact app-name searches selected Notion ahead of a recent Code fuzzy match on all panels."
        );
    }

    fn verify_usage_hint_visibility(mtm: MainThreadMarker) {
        let delegate = Delegate::new(mtm);
        let state = delegate.ivars();
        state.config.borrow_mut().display_density = DisplayDensity::Compact;
        state.demo.set(true);
        state.windows.replace(
            (0..10)
                .map(|id| WindowInfo {
                    id,
                    pid: -1,
                    app: "Browser".into(),
                    title: format!("Window {id}"),
                    minimized: false,
                })
                .collect(),
        );
        delegate.sync_displays();
        for mode in [PanelMode::Search, PanelMode::Switch] {
            state.mode.set(Some(mode));
            for show_hints in [true, false, true, false] {
                state.config.borrow_mut().show_usage_hints = show_hints;
                delegate.filter();
                for ui in delegate.panels() {
                    assert_eq!(ui.footer.isHidden(), !show_hints);
                    assert_eq!(
                        ui.mode_label.isHidden(),
                        mode != PanelMode::Switch || !show_hints
                    );
                    let root = ui.panel.contentView().unwrap();
                    let settings = root
                        .subviews()
                        .into_iter()
                        .filter_map(|view| view.downcast::<NSButton>().ok())
                        .find(|button| button.action() == Some(sel!(showSettings:)))
                        .unwrap();
                    assert_eq!(settings.isHidden(), !show_hints);
                    assert!(settings.frame().origin.y >= 0.0);
                    assert!(
                        settings.frame().origin.y + settings.frame().size.height
                            <= ui.scroll.frame().origin.y
                    );
                    assert_eq!(
                        root.bounds().size.height,
                        if mode == PanelMode::Switch {
                            388.0 + if show_hints { MODE_LABEL_SPACING } else { 0.0 }
                        } else {
                            416.0
                        }
                    );
                    assert!(!ui.panel.isVisible());
                }
            }
        }
        state.alias_input.borrow_mut().push('z');
        delegate.render();
        for ui in delegate.panels() {
            assert!(
                !ui.mode_label.isHidden(),
                "typed alias feedback must stay visible"
            );
            assert!(ui.mode_label.stringValue().to_string().contains("Alias  z"));
            assert!(ui.footer.isHidden());
        }
        state.alias_input.borrow_mut().clear();
        delegate.render();
        assert!(delegate.panels().iter().all(|ui| ui.mode_label.isHidden()));
        state.demo.set(false);
        state
            .hotkey_error
            .replace(Some("Shortcut unavailable".into()));
        delegate.render();
        for ui in delegate.panels() {
            assert!(!ui.footer.isHidden());
            assert_eq!(ui.footer.stringValue().to_string(), "Shortcut unavailable");
        }
        state.hotkey_error.replace(None);
        state.alias_error.replace(Some("Alias unavailable".into()));
        delegate.render();
        for ui in delegate.panels() {
            assert!(!ui.footer.isHidden());
            assert_eq!(ui.footer.stringValue().to_string(), "Alias unavailable");
        }
        state.alias_error.replace(None);
        for mode in [PanelMode::Search, PanelMode::Switch] {
            state.mode.set(Some(mode));
            for loading in [false, true] {
                state.loading.set(loading);
                for show_hints in [true, false] {
                    state.config.borrow_mut().show_usage_hints = show_hints;
                    delegate.render();
                    for ui in delegate.panels() {
                        assert_eq!(
                            ui.footer.isHidden(),
                            !show_hints && accessibility::is_trusted(),
                            "refreshing must respect the footer toggle; permission errors stay visible"
                        );
                    }
                }
            }
        }
        state.loading.set(false);
        state.demo.set(true);
        delegate.render();
        delegate.report_switch_error("Switch failed");
        for ui in delegate.panels() {
            assert!(!ui.footer.isHidden());
            assert_eq!(ui.footer.stringValue().to_string(), "Switch failed");
        }
        println!(
            "Usage hint checks passed: both modes, all displays, compact layout, settings access, alias feedback and errors."
        );
    }

    fn verify_display_density(mtm: MainThreadMarker) {
        let delegate = Delegate::new(mtm);
        let state = delegate.ivars();
        state.demo.set(true);
        state.windows.replace(
            (0..24)
                .map(|id| WindowInfo {
                    id,
                    pid: -1,
                    app: "Editor".into(),
                    title: format!("Project {id}"),
                    minimized: false,
                })
                .collect(),
        );
        delegate.sync_displays();
        for mode in [PanelMode::Search, PanelMode::Switch] {
            state.mode.set(Some(mode));
            state.query.replace(String::new());
            delegate.filter();
            state.selected.set(23);
            for (density, pitch, icon_size, font_size) in [
                (DisplayDensity::Compact, 28.0, 20.0, 13.0),
                (DisplayDensity::Normal, 32.0, 24.0, 15.0),
                (DisplayDensity::Compact, 28.0, 20.0, 13.0),
            ] {
                state.config.borrow_mut().display_density = density;
                delegate.render();
                for ui in delegate.panels() {
                    let rows = ui.rows.borrow();
                    assert_eq!(
                        rows[1].button.frame().origin.y - rows[0].button.frame().origin.y,
                        pitch
                    );
                    assert_eq!(rows[0].icon.frame().size.width, icon_size);
                    assert_eq!(rows[0].title.font().unwrap().pointSize(), font_size);
                    for row in rows.iter() {
                        let center = row.button.bounds().size.height / 2.0;
                        for view in [&*row.alias as &NSView, &*row.icon as &NSView] {
                            let frame = view.frame();
                            assert!(
                                (frame.origin.y + frame.size.height / 2.0 - center).abs() < 0.01
                            );
                        }
                        for child in row.button.subviews() {
                            let frame = child.frame();
                            assert!(frame.origin.y >= 0.0);
                            assert!(
                                frame.origin.y + frame.size.height
                                    <= row.button.bounds().size.height
                            );
                            assert!(
                                frame.origin.x + frame.size.width <= row.button.bounds().size.width
                            );
                        }
                    }
                    let selected = rows[23].button.frame();
                    let viewport = ui.scroll.contentView().bounds();
                    assert!(selected.origin.y >= viewport.origin.y);
                    assert!(
                        selected.origin.y + selected.size.height
                            <= viewport.origin.y + viewport.size.height
                    );
                    assert!(rows[23].button.isAccessibilitySelected());
                    assert!(ui.panel.contentView().unwrap().bounds().size.height <= HEIGHT);
                    assert!(!ui.panel.isVisible());
                }
                let before: Vec<_> = delegate
                    .panels()
                    .iter()
                    .map(|ui| ui.list.subviews())
                    .collect();
                delegate.render();
                for (ui, rows) in delegate.panels().iter().zip(before) {
                    assert_eq!(
                        ui.list.subviews(),
                        rows,
                        "unchanged density must reuse rows"
                    );
                }
            }
            let mut heights = Vec::new();
            state.query.replace("Project 1".into());
            for density in [DisplayDensity::Compact, DisplayDensity::Normal] {
                state.config.borrow_mut().display_density = density;
                delegate.filter();
                heights.push(
                    delegate.panels()[0]
                        .panel
                        .contentView()
                        .unwrap()
                        .bounds()
                        .size
                        .height,
                );
            }
            assert!(
                heights[1] > heights[0],
                "normal rows must expand the adaptive panel"
            );
        }
        println!(
            "Density checks passed: live Compact/Normal changes, centered aliases/icons, scrolling, selection, row reuse and all displays."
        );
    }

    fn verify_adaptive_panels(mtm: MainThreadMarker) {
        let delegate = Delegate::new(mtm);
        let state = delegate.ivars();
        state.config.borrow_mut().display_density = DisplayDensity::Compact;
        state.demo.set(true);
        for mode in [PanelMode::Search, PanelMode::Switch] {
            state.mode.set(Some(mode));
            let mut heights = Vec::new();
            let mut top_edges = Vec::new();
            for count in [24, 14, 7, 1, 0] {
                state.windows.replace(
                    (0..count)
                        .map(|id| WindowInfo {
                            id,
                            pid: -1,
                            app: "Browser".into(),
                            title: format!("Window {id}"),
                            minimized: false,
                        })
                        .collect(),
                );
                delegate.filter();
                if state.panels.borrow().is_empty() {
                    delegate.sync_displays();
                    delegate.render();
                }
                for (index, ui) in delegate.panels().iter().enumerate() {
                    let frame = ui.panel.frame();
                    let root = ui.panel.contentView().unwrap();
                    let top = frame.origin.y + frame.size.height;
                    if count == 24 {
                        top_edges.push(top);
                    }
                    assert!(
                        (top - top_edges[index]).abs() < 1.0,
                        "filtering must preserve the panel's top edge"
                    );
                    assert!(
                        ui.scroll.frame().origin.y
                            >= ui.footer.frame().origin.y + ui.footer.frame().size.height
                    );
                    if mode == PanelMode::Search {
                        assert!(
                            ui.input.frame().origin.y + ui.input.frame().size.height
                                <= root.bounds().size.height
                        );
                        assert!(
                            ui.scroll.frame().origin.y + ui.scroll.frame().size.height
                                <= ui.input.frame().origin.y - 8.0
                        );
                    }
                    for label in ui.empty_labels.borrow().iter() {
                        assert!(label.frame().origin.y >= 0.0);
                        assert!(
                            label.frame().origin.y + label.frame().size.height
                                <= ui.scroll.frame().size.height,
                            "empty-state text must fit without scrolling"
                        );
                    }
                    assert!(!ui.panel.isVisible());
                }
                heights.push(
                    delegate.panels()[0]
                        .panel
                        .contentView()
                        .unwrap()
                        .bounds()
                        .size
                        .height,
                );
            }
            assert!(
                heights[1] < heights[0],
                "fourteen items should be shorter than the capped list"
            );
            assert!(heights[2] < heights[1]);
            assert!(heights[3] < heights[2]);
        }
        println!(
            "Adaptive panel checks passed: two modes, 0/1/7/14/24 items, fixed top edge and visible controls."
        );
    }

    fn verify_alias_does_not_block_title_search(mtm: MainThreadMarker) {
        let delegate = Delegate::new(mtm);
        let state = delegate.ivars();
        state.demo.set(true);
        state.mode.set(Some(PanelMode::Search));
        state.automatic_aliases.replace(
            Aliases::from_json(r#"{"com.apple.finder":"fi","com.microsoft.VSCode":"co"}"#).unwrap(),
        );
        state.identities.replace(HashMap::from([
            (
                -10,
                AppIdentity {
                    id: "com.microsoft.VSCode".into(),
                    english_name: "Code".into(),
                },
            ),
            (
                -20,
                AppIdentity {
                    id: "com.apple.finder".into(),
                    english_name: "Finder".into(),
                },
            ),
        ]));
        let fiber = WindowInfo {
            id: 1,
            pid: -10,
            app: "Code".into(),
            title: "channel.rs — fiber".into(),
            minimized: false,
        };
        let other = WindowInfo {
            id: 2,
            pid: -10,
            app: "Code".into(),
            title: "main.rs — rust".into(),
            minimized: false,
        };
        let finder = WindowInfo {
            id: 3,
            pid: -20,
            app: "Finder".into(),
            title: "Documents".into(),
            minimized: false,
        };
        delegate.install_windows(vec![other.clone(), fiber.clone()]);
        delegate.sync_displays();
        for query in ["f", "fi", "fib", "fiber", " FI "] {
            state.query.replace(query.into());
            delegate.filter();
            assert_eq!(
                delegate.match_count(),
                1,
                "a saved alias without a window must not block {query}"
            );
            assert_eq!(delegate.selected_window().unwrap().id, 1);
        }
        state.query.replace("fi".into());
        delegate.install_windows(vec![other.clone(), fiber.clone(), finder]);
        delegate.filter();
        assert_eq!(
            delegate.match_count(),
            2,
            "a live alias must retain the matching project below it"
        );
        assert_eq!(delegate.selected_window().unwrap().id, 3);
        delegate.move_selection(1);
        assert_eq!(delegate.selected_window().unwrap().id, 1);
        delegate.filter_preserving(delegate.selected_result());
        assert_eq!(delegate.selected_window().unwrap().id, 1);
        for ui in delegate.panels() {
            let rows = ui.rows.borrow();
            assert_eq!(rows[1].title.stringValue().to_string(), "fiber: channel.rs");
            assert!(rows[1].button.isAccessibilitySelected());
            assert!(!ui.panel.isVisible());
        }
        state.config.borrow_mut().excluded_apps = vec!["Finder".into()];
        delegate.filter();
        assert_eq!(delegate.match_count(), 1);
        assert_eq!(delegate.selected_window().unwrap().id, 1);
        state.config.borrow_mut().excluded_apps = vec!["Code".into()];
        delegate.filter();
        assert_eq!(delegate.match_count(), 1);
        assert_eq!(delegate.selected_window().unwrap().id, 3);
        state.config.borrow_mut().excluded_apps.clear();
        delegate.install_windows(vec![other, fiber]);
        delegate.filter();
        assert_eq!(delegate.match_count(), 1);
        assert_eq!(delegate.selected_window().unwrap().id, 1);
        println!(
            "Alias/title search checks passed: fi finds fiber with Finder absent, present, excluded, and closed; selection survives refresh."
        );
    }

    fn verify_distinct_window_aliases(mtm: MainThreadMarker) {
        let delegate = Delegate::new(mtm);
        let state = delegate.ivars();
        state.demo.set(true);
        state
            .automatic_aliases
            .replace(Aliases::from_json(r#"{"code":"co"}"#).unwrap());
        state.identities.borrow_mut().insert(
            -42,
            AppIdentity {
                id: "code".into(),
                english_name: "Code".into(),
            },
        );
        let windows: Vec<_> = (1..=3)
            .map(|id| WindowInfo {
                id,
                pid: -42,
                app: "Code".into(),
                title: format!("Project {id}"),
                minimized: false,
            })
            .collect();
        delegate.update_aliases(&windows);
        state.windows.replace(windows);
        state.mode.set(Some(PanelMode::Search));
        delegate.sync_displays();
        delegate.filter();
        let expected: Vec<_> = delegate.panels()[0]
            .rows
            .borrow()
            .iter()
            .map(|row| {
                let RowContent::Window(window, _) = row.content.as_ref().unwrap() else {
                    panic!("expected a window")
                };
                (window.id, row.alias.stringValue().to_string())
            })
            .collect();
        assert_eq!(expected.len(), 3);
        assert!(expected.iter().all(|(_, alias)| !alias.is_empty()));
        assert_eq!(
            expected
                .iter()
                .map(|(_, alias)| alias)
                .collect::<HashSet<_>>()
                .len(),
            3,
            "each independent Code window needs a distinct alias"
        );
        for (id, alias) in &expected {
            state.mode.set(Some(PanelMode::Search));
            state.recency.replace(vec![3, 1, 2]);
            state.query.replace(alias.clone());
            delegate.filter();
            assert_eq!(state.matches.borrow().len(), 3);
            assert_eq!(delegate.selected_window().unwrap().id, *id);
            let mut ordered_ids = vec![*id];
            ordered_ids.extend([3, 1, 2].into_iter().filter(|other| other != id));
            assert_eq!(
                state
                    .matches
                    .borrow()
                    .iter()
                    .map(|&index| state.windows.borrow()[index].id)
                    .collect::<Vec<_>>(),
                ordered_ids,
                "the exact alias leads, followed by sibling windows in recent order"
            );
            for ui in delegate.panels() {
                let rows = ui.rows.borrow();
                assert!(rows[0].button.isAccessibilitySelected());
                assert!(rows.iter().take(3).all(|row| row.attached));
            }
            delegate.move_selection(1);
            assert_eq!(delegate.selected_window().unwrap().id, ordered_ids[1]);
            delegate.filter_preserving(delegate.selected_result());
            assert_eq!(delegate.selected_window().unwrap().id, ordered_ids[1]);
            state.query.borrow_mut().clear();
            state.mode.set(Some(PanelMode::Switch));
            state
                .switch_selection
                .replace(Some(SwitchSelection::new(0)));
            state.alias_input.borrow_mut().clear();
            delegate.filter();
            delegate.prepare_switch_selection();
            for ch in alias.chars() {
                delegate.shortcut_action(Action {
                    session: state.session.get(),
                    kind: ActionKind::Alias(ch),
                });
            }
            assert_eq!(delegate.selected_window().unwrap().id, *id);
            for ui in delegate.panels() {
                let row = &ui.rows.borrow()[state.selected.get()];
                assert_eq!(row.alias.stringValue().to_string(), *alias);
                assert!(row.button.isAccessibilitySelected());
            }
            delegate.shortcut_action(Action {
                session: state.session.get(),
                kind: ActionKind::Accept,
            });
            assert!(delegate.panels().iter().all(|ui| {
                ui.footer
                    .stringValue()
                    .to_string()
                    .contains(&format!("Project {id}（"))
            }));
        }

        state.mode.set(Some(PanelMode::Switch));
        state
            .switch_selection
            .replace(Some(SwitchSelection::new(0)));
        state.alias_input.borrow_mut().clear();
        delegate.filter();
        delegate.prepare_switch_selection();
        let before = state.aliases.borrow().clone();
        let mut pending = state.windows.borrow().clone();
        pending.retain(|window| window.id != 1);
        pending.push(WindowInfo {
            id: 4,
            pid: -42,
            app: "Code".into(),
            title: "New project".into(),
            minimized: false,
        });
        state.deferred_windows.replace(Some(pending));
        delegate.render();
        assert_eq!(*state.aliases.borrow(), before);
        delegate.display_search(state.session.get());
        let after = state.aliases.borrow();
        assert_eq!(after.for_window(1), None);
        assert!(after.for_window(4).is_some());
        for id in [2, 3] {
            assert_eq!(after.for_window(id), before.for_window(id));
        }
    }

    fn verify_switch_alias_prefix(mtm: MainThreadMarker) {
        let delegate = Delegate::new(mtm);
        let state = delegate.ivars();
        state.demo.set(true);
        state.session.set(7);
        state
            .automatic_aliases
            .replace(Aliases::from_json(r#"{"zed":"z","zulip":"zu","zoom":"zo"}"#).unwrap());
        state.windows.replace(vec![
            WindowInfo {
                id: 41,
                pid: -41,
                app: "Code".into(),
                title: "Project".into(),
                minimized: false,
            },
            WindowInfo {
                id: 42,
                pid: -42,
                app: "Zulip".into(),
                title: "Messages".into(),
                minimized: false,
            },
        ]);
        state.identities.borrow_mut().insert(
            -42,
            AppIdentity {
                id: "zulip".into(),
                english_name: "Zulip".into(),
            },
        );
        delegate.update_aliases(&state.windows.borrow());
        state.mode.set(Some(PanelMode::Switch));
        state
            .switch_selection
            .replace(Some(SwitchSelection::new(0)));
        delegate.sync_displays();
        delegate.filter();
        delegate.prepare_switch_selection();
        delegate.shortcut_action(Action {
            session: 7,
            kind: ActionKind::Alias('z'),
        });
        assert_eq!(delegate.selected_window().unwrap().id, 42);
        for ui in delegate.panels() {
            assert!(!ui.mode_label.stringValue().to_string().contains("没有匹配"));
            assert!(
                ui.rows.borrow()[state.selected.get()]
                    .button
                    .isAccessibilitySelected()
            );
        }
        delegate.shortcut_action(Action {
            session: 7,
            kind: ActionKind::Accept,
        });
        assert_eq!(state.mode.get(), Some(PanelMode::Search));
        assert!(delegate.panels().iter().all(|ui| {
            ui.footer
                .stringValue()
                .to_string()
                .contains("演示选择：Zulip")
        }));

        state.windows.borrow_mut().push(WindowInfo {
            id: 43,
            pid: -43,
            app: "Zoom".into(),
            title: "Meeting".into(),
            minimized: false,
        });
        state.identities.borrow_mut().insert(
            -43,
            AppIdentity {
                id: "zoom".into(),
                english_name: "Zoom".into(),
            },
        );
        delegate.update_aliases(&state.windows.borrow());
        state.mode.set(Some(PanelMode::Switch));
        state
            .switch_selection
            .replace(Some(SwitchSelection::new(0)));
        delegate.filter();
        delegate.prepare_switch_selection();
        delegate.shortcut_action(Action {
            session: 7,
            kind: ActionKind::Alias('z'),
        });
        assert_eq!(delegate.alias_match(), AliasMatch::Ambiguous);
        for ui in delegate.panels() {
            assert!(
                ui.mode_label
                    .stringValue()
                    .to_string()
                    .contains("继续输入第二个字母")
            );
            assert!(
                ui.rows
                    .borrow()
                    .iter()
                    .all(|row| !row.button.isAccessibilitySelected())
            );
        }
        delegate.shortcut_action(Action {
            session: 7,
            kind: ActionKind::Alias('u'),
        });
        assert_eq!(delegate.selected_window().unwrap().id, 42);
        delegate.shortcut_action(Action {
            session: 7,
            kind: ActionKind::AliasBackspace,
        });
        assert_eq!(delegate.alias_match(), AliasMatch::Ambiguous);
        delegate.shortcut_action(Action {
            session: 7,
            kind: ActionKind::Accept,
        });
        assert_eq!(
            state.mode.get(),
            None,
            "ambiguous release must cancel instead of committing the previous selection"
        );
        assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));
    }

    fn verify_main_wake(mtm: MainThreadMarker) {
        use core_foundation::runloop::{CFRunLoop, kCFRunLoopDefaultMode};
        let received = Rc::new(RefCell::new(Vec::new()));
        let captured = received.clone();
        let calls = Rc::new(Cell::new(0));
        let callback_calls = calls.clone();
        let (tx, rx) = mpsc::channel();
        let wake = MainWake::new(mtm, move || {
            assert!(MainThreadMarker::new().is_some());
            callback_calls.set(callback_calls.get() + 1);
            captured.borrow_mut().extend(rx.try_iter());
        });
        let handle = wake.handle();
        let worker_handle = handle.clone();
        std::thread::spawn(move || {
            for value in 0..100 {
                tx.send(value).unwrap();
                worker_handle.signal();
            }
        })
        .join()
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while received.borrow().len() < 100 && Instant::now() < deadline {
            CFRunLoop::run_in_mode(
                unsafe { kCFRunLoopDefaultMode },
                Duration::from_millis(10),
                true,
            );
        }
        assert_eq!(*received.borrow(), (0..100).collect::<Vec<_>>());
        let before_drop = calls.get();
        drop(wake);
        handle.signal();
        CFRunLoop::run_in_mode(
            unsafe { kCFRunLoopDefaultMode },
            Duration::from_millis(5),
            false,
        );
        assert_eq!(received.borrow().len(), 100);
        assert_eq!(calls.get(), before_drop);
    }

    fn export_hidden_previews(delegate: &Delegate, directory: &str) {
        std::fs::create_dir_all(directory).unwrap();
        let state = delegate.ivars();
        state.query.replace(String::new());
        state.current_app_only.set(false);
        state.alias_input.borrow_mut().clear();
        let windows: Vec<_> = demo_windows()
            .into_iter()
            .cycle()
            .take(
                std::env::var("WINLANE_PREVIEW_WINDOWS")
                    .ok()
                    .and_then(|count| count.parse().ok())
                    .unwrap_or(22),
            )
            .enumerate()
            .map(|(index, mut window)| {
                window.id = index as u64;
                window
            })
            .collect();
        for window in &windows {
            let app = AppIdentity {
                id: format!("example.{}", window.app),
                english_name: window.app.clone(),
            };
            state.identities.borrow_mut().insert(window.pid, app);
        }
        delegate.install_windows(windows);
        state.selected.set(1);
        let ui = delegate.panels()[0].clone();
        // SAFETY: AppKit supplies immutable appearance-name constants.
        let appearances = unsafe {
            [
                ("light", NSAppearanceNameAqua),
                ("dark", NSAppearanceNameDarkAqua),
            ]
        };
        for (density_name, density) in [
            ("compact", DisplayDensity::Compact),
            ("normal", DisplayDensity::Normal),
        ] {
            state.config.borrow_mut().display_density = density;
            for (name, appearance) in &appearances {
                ui.panel
                    .setAppearance(NSAppearance::appearanceNamed(appearance).as_deref());
                for (mode_name, mode) in
                    [("search", PanelMode::Search), ("switch", PanelMode::Switch)]
                {
                    state.mode.set(Some(mode));
                    delegate.filter();
                    delegate.move_selection(1);
                    let root = ui.panel.contentView().unwrap();
                    root.layoutSubtreeIfNeeded();
                    let bitmap = root
                        .bitmapImageRepForCachingDisplayInRect(root.bounds())
                        .unwrap();
                    root.cacheDisplayInRect_toBitmapImageRep(root.bounds(), &bitmap);
                    // SAFETY: The empty dictionary does not supply any typed image properties.
                    let png = unsafe {
                        bitmap.representationUsingType_properties(
                            NSBitmapImageFileType::PNG,
                            &objc2_foundation::NSDictionary::new(),
                        )
                    }
                    .unwrap();
                    let path = format!("{directory}/{density_name}-{name}-{mode_name}.png");
                    assert!(png.writeToFile_atomically(&NSString::from_str(&path), true));
                    assert!(!ui.panel.isVisible());
                }
            }
        }
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    objc2::rc::autoreleasepool(|_| {
        winlane::i18n::set_locale(winlane::i18n::Locale::Chinese);
        if let Ok(expectation) = std::env::var("WINLANE_UPDATER_SMOKE") {
            updater::verify(&expectation);
        } else if std::env::var_os("WINLANE_LAUNCH_SMOKE").is_some() {
            app_shortcuts::verify_background_launch();
        } else if std::env::var_os("WINLANE_APP_SCAN").is_some() {
            let start = std::time::Instant::now();
            let apps = installed_apps::discover();
            assert!(!apps.is_empty());
            assert_eq!(
                apps.iter()
                    .map(|app| &app.target.bundle_id)
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                apps.len()
            );
            println!(
                "Installed application scan: {} apps in {:?}; no applications launched.",
                apps.len(),
                start.elapsed()
            );
        } else if let Ok(bundle) = std::env::var("WINLANE_SCAN_BUNDLE") {
            app::inspect_window_discovery(&bundle);
        } else if std::env::var_os("WINLANE_BENCHMARK").is_some() {
            app::benchmark_hidden_panels();
        } else {
            app::verify_hidden_panels();
        }
    });
    #[cfg(not(target_os = "macos"))]
    println!("Native panel checks require macOS.");
}

#[cfg(target_os = "macos")]
mod snippet_placeholder {
    include!("../src/snippet_placeholder.rs");
    include!("support/snippet_placeholder.rs");
}

#[cfg(target_os = "macos")]
#[path = "../src/clipboard_runtime.rs"]
mod clipboard_runtime;

#[cfg(target_os = "macos")]
#[path = "../src/clipboard_settings.rs"]
mod clipboard_settings;

#[cfg(target_os = "macos")]
#[path = "../src/clipboard_image.rs"]
mod clipboard_image;
