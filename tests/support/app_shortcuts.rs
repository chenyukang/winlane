pub fn verify_hidden_settings(target: &AnyObject, mtm: MainThreadMarker) {
    let window = AppShortcutsWindow::new(target, mtm);
    assert!(!window.window.isVisible());
    assert_eq!(window.candidate().unwrap(), []);
    window.add(target, mtm);
    assert!(window.candidate().is_err());
    window.rows.borrow()[0].set_application(ApplicationTarget {
        bundle_id: "com.example.browser".into(),
        path: "/Applications/Example Browser.app".into(),
        name: "Browser".into(),
    });
    window.add(target, mtm);
    window.rows.borrow()[1].set_application(ApplicationTarget {
        bundle_id: "com.example.chat".into(),
        path: "/Applications/Example Chat.app".into(),
        name: "Discord".into(),
    });
    let items = window.candidate().unwrap();
    assert_eq!(items[0].shortcut.display(), "⌘1");
    assert_eq!(items[1].shortcut.display(), "⌘2");
    let config = winlane::config::Config {
        app_shortcuts: items.clone(),
        ..Default::default()
    };
    config.validate().unwrap();
    let general = crate::settings::SettingsWindow::new(target, mtm);
    general.fill(&config);
    assert_eq!(general.candidate().unwrap(), config);
    window.remove(0);
    assert_eq!(window.candidate().unwrap(), items[1..]);
    assert_eq!(window.rows.borrow()[0].choose.tag(), 0);
    window.fill(&items, target, mtm);
    assert_eq!(window.candidate().unwrap(), items);
    assert!(!window.window.isVisible());
    window.add(target, mtm);
    let translated = AppShortcutsWindow::new(target, mtm);
    translated.copy_draft_from(&window, target, mtm);
    assert_eq!(translated.rows.borrow().len(), 3);
    assert!(translated.rows.borrow()[2].application.borrow().is_none());
    assert_eq!(translated.rows.borrow()[2].shortcut.read().unwrap().display(), "⌘3");
    translated.remove(2);
    assert_eq!(translated.candidate().unwrap(), items);
}

pub fn verify_background_launch() {
    use core_foundation::runloop::{CFRunLoop, kCFRunLoopDefaultMode};
    use std::time::{Duration, Instant};
    let mtm = MainThreadMarker::new().unwrap();
    NSApplication::sharedApplication(mtm)
        .setActivationPolicy(NSApplicationActivationPolicy::Prohibited);
    let directory =
        std::env::temp_dir().join(format!("winlane-launch-fixture-{}", std::process::id()));
    let bundle = directory.join("Shortcut Fixture.app");
    let executable = bundle.join("Contents/MacOS/fixture");
    std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
    let identifier = format!(
        "com.example.winlane-shortcut-fixture-{}",
        std::process::id()
    );
    std::fs::write(bundle.join("Contents/Info.plist"), format!(r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>{identifier}</string><key>CFBundleName</key><string>Shortcut Fixture</string><key>CFBundleExecutable</key><string>fixture</string><key>CFBundlePackageType</key><string>APPL</string><key>LSBackgroundOnly</key><true/></dict></plist>"#)).unwrap();
    let source = directory.join("fixture.swift");
    std::fs::write(&source, "import AppKit\nlet app = NSApplication.shared\napp.setActivationPolicy(.prohibited)\napp.run()\n").unwrap();
    assert!(
        std::process::Command::new("swiftc")
            .args([
                source.as_os_str(),
                std::ffi::OsStr::new("-o"),
                executable.as_os_str()
            ])
            .status()
            .unwrap()
            .success()
    );
    let url = NSURL::fileURLWithPath(&NSString::from_str(bundle.to_str().unwrap()));
    let target = target_at_url(&url).unwrap();
    assert_eq!(target.bundle_id, identifier);
    assert_eq!(resolve_application(&target).unwrap().path(), url.path());
    let wrong = ApplicationTarget {
        bundle_id: format!("{identifier}.wrong"),
        ..target.clone()
    };
    assert!(
        resolve_application(&wrong).is_err(),
        "must not open a different app at the old path"
    );
    let wake = crate::main_wake::MainWake::new(mtm, || {});
    let wait_for = |rx: Receiver<Result<i32, String>>| {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Ok(result) = rx.try_recv() {
                break result.unwrap();
            }
            assert!(Instant::now() < deadline, "launch callback timed out");
            CFRunLoop::run_in_mode(
                unsafe { kCFRunLoopDefaultMode },
                Duration::from_millis(10),
                false,
            );
        }
    };
    let pid = wait_for(crate::app::launch_search_fixture(target.clone()));
    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid).unwrap();
    assert_eq!(app.bundleIdentifier().unwrap().to_string(), identifier);
    let second = wait_for(launch(&target, wake.handle()).unwrap());
    assert_eq!(
        pid, second,
        "opening an already running app must reuse its process"
    );
    assert!(
        NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .is_none_or(|front| front.processIdentifier() != pid)
    );
    assert!(app.terminate());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !app.isTerminated() && Instant::now() < deadline {
        CFRunLoop::run_in_mode(
            unsafe { kCFRunLoopDefaultMode },
            Duration::from_millis(10),
            false,
        );
    }
    assert!(app.isTerminated());
    std::fs::remove_dir_all(&directory).unwrap();
    println!(
        "Application launch checks passed: launch from stopped, reuse running process, reject wrong bundle; fixture stayed in the background and was removed."
    );
}
