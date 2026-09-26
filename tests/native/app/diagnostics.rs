use super::*;

pub(crate) fn inspect_window_discovery(bundle: &str) {
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
            crate::macos::platform::accessibility::tests::inspect_published_window_changes(*pid);
            crate::macos::platform::accessibility::tests::inspect_remembered_windows_stay_listed(
                *pid,
            );
        }
        return;
    }
    for round in 0..12 {
        let start = Instant::now();
        let (windows, _) = accessibility::list_windows(&apps, true);
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

pub(crate) fn benchmark_hidden_panels() {
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

pub(super) fn export_hidden_previews(delegate: &Delegate, directory: &str) {
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
                let path = format!("{directory}/{density_name}-{name}-{mode_name}.png");
                assert!(png.writeToFile_atomically(&NSString::from_str(&path), true));
                assert!(!ui.panel.isVisible());
            }
        }
    }
}
