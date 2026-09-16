#![allow(dead_code)]

#[cfg(target_os = "macos")]
#[path = "../src/main_wake.rs"]
mod main_wake;

#[cfg(target_os = "macos")]
#[path = "../src/accessibility.rs"]
mod accessibility;
#[cfg(target_os = "macos")]
#[path = "../src/settings.rs"]
mod settings;
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
        let after = memory_sample();
        assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));
        println!(
            "{}",
            serde_json::json!({
                "windows":count, "displays":delegate.panels().len(),
                "panel_init_ms":panel_init_ms, "first_list_ms":first_list_ms,
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
        let delegate = Delegate::new(mtm);
        delegate.ivars().demo.set(true);
        delegate.ivars().windows.replace(demo_windows());
        delegate.ivars().mode.set(Some(PanelMode::Search));
        delegate.sync_displays();
        delegate.filter();
        let panels = delegate.panels();
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
        let scope = &panels.last().unwrap().scope;
        scope.setState(NSControlStateValueOn);
        unsafe {
            let _: () = msg_send![&*delegate, changeScope: &**scope];
        }
        assert!(
            panels
                .iter()
                .all(|ui| ui.scope.state() == NSControlStateValueOn)
        );
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

    fn verify_switch_alias_prefix(mtm: MainThreadMarker) {
        let delegate = Delegate::new(mtm);
        let state = delegate.ivars();
        state.demo.set(true);
        state.session.set(7);
        state
            .aliases
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
            .take(22)
            .enumerate()
            .map(|(index, mut window)| {
                window.id = index as u64;
                window
            })
            .collect();
        let apps: Vec<_> = windows
            .iter()
            .map(|window| {
                let app = AppIdentity {
                    id: format!("example.{}", window.app),
                    english_name: window.app.clone(),
                };
                state
                    .identities
                    .borrow_mut()
                    .insert(window.pid, app.clone());
                app
            })
            .collect();
        state.aliases.borrow_mut().ensure(&apps);
        state.windows.replace(windows);
        state.selected.set(1);
        let ui = delegate.panels()[0].clone();
        // SAFETY: AppKit supplies immutable appearance-name constants.
        let appearances = unsafe {
            [
                ("light", NSAppearanceNameAqua),
                ("dark", NSAppearanceNameDarkAqua),
            ]
        };
        for (name, appearance) in appearances {
            ui.panel
                .setAppearance(NSAppearance::appearanceNamed(appearance).as_deref());
            for (mode_name, mode) in [("search", PanelMode::Search), ("switch", PanelMode::Switch)]
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
                let path = format!("{directory}/{name}-{mode_name}.png");
                assert!(png.writeToFile_atomically(&NSString::from_str(&path), true));
                assert!(!ui.panel.isVisible());
            }
        }
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    objc2::rc::autoreleasepool(|_| {
        if let Ok(bundle) = std::env::var("WINLANE_SCAN_BUNDLE") {
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
