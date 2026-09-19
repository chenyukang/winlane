fn verify_recency_persistence(mtm: MainThreadMarker) {
    use objc2::AnyThread;

    let domain = NSString::from_str(&format!(
        "com.example.winlane-recency-{}",
        std::process::id()
    ));
    let defaults =
        NSUserDefaults::initWithSuiteName(NSUserDefaults::alloc(), Some(&domain)).unwrap();
    defaults.removePersistentDomainForName(&domain);
    let config = Config::default();
    settings::save(&config, &defaults).unwrap();
    let key = ns_string!("WinlaneRecentWindowsV1");
    // SAFETY: This notification exercises the normal termination callback without quitting the test.
    let quit = unsafe {
        NSNotification::notificationWithName_object(
            ns_string!("NSApplicationWillTerminateNotification"),
            None,
        )
    };
    let saved = || {
        serde_json::from_str::<Vec<u64>>(&defaults.stringForKey(key).unwrap().to_string()).unwrap()
    };
    let order = |delegate: &Delegate| {
        delegate.filter();
        delegate
            .ivars()
            .matches
            .borrow()
            .iter()
            .map(|&i| delegate.ivars().windows.borrow()[i].id)
            .collect::<Vec<_>>()
    };

    let delegate = responsive_fixture(mtm);
    delegate.restore_recency(defaults.clone());
    assert!(delegate.ivars().recency.borrow().is_empty());
    for id in [3, 1, 2] {
        delegate.remember_window(id);
    }
    assert!(
        defaults.stringForKey(key).is_none(),
        "switches must not save history"
    );
    assert_eq!(order(&delegate), [2, 1, 3]);
    let revision = delegate.ivars().focus_revision.get();
    delegate.remember_window(2);
    assert!(defaults.stringForKey(key).is_none());
    assert_ne!(delegate.ivars().focus_revision.get(), revision);
    delegate.will_terminate(sel!(applicationWillTerminate:), &quit);
    assert_eq!(saved(), [2, 1, 3]);
    drop(delegate);

    let delegate = responsive_fixture(mtm);
    let reopened =
        NSUserDefaults::initWithSuiteName(NSUserDefaults::alloc(), Some(&domain)).unwrap();
    delegate.restore_recency(reopened);
    assert_eq!(order(&delegate), [2, 1, 3]);
    let state = delegate.ivars();
    let missing = state.windows.borrow_mut().remove(1);
    assert_eq!(order(&delegate), [1, 3]);
    assert_eq!(
        saved(),
        [2, 1, 3],
        "a temporary scan omission must not erase history"
    );
    state.windows.borrow_mut().push(missing);
    assert_eq!(order(&delegate), [2, 1, 3]);

    delegate.remember_window(3); // Capture the current foreground after restoration.
    assert_eq!(order(&delegate), [3, 2, 1]);
    assert_eq!(saved(), [2, 1, 3]);
    let tx = pending_test_focus(&delegate, -3);
    delegate.remember_window(1); // Provisional cached window.
    state.focus_receiver.borrow_mut().as_mut().unwrap().revision = state.focus_revision.get();
    assert_eq!(saved(), [2, 1, 3]);
    tx.send(Some(3)).unwrap();
    delegate.poll_focus();
    assert_eq!(*state.recency.borrow(), [3, 2, 1]);
    assert_eq!(saved(), [2, 1, 3]);
    let tx = pending_test_focus(&delegate, -1);
    delegate.remember_window(2);
    tx.send(Some(1)).unwrap();
    delegate.poll_focus();
    assert_eq!(*state.recency.borrow(), [2, 3, 1]);
    assert_eq!(
        saved(),
        [2, 1, 3],
        "background focus must not write history"
    );
    delegate.will_terminate(sel!(applicationWillTerminate:), &quit);
    assert_eq!(saved(), [2, 3, 1], "save the final corrected order on exit");
    assert_eq!(
        Config::from_json(
            &defaults
                .stringForKey(ns_string!("WindowlanePreferencesV1"))
                .unwrap()
                .to_string()
        )
        .unwrap(),
        config,
        "saving history must not alter user settings"
    );

    for id in 1..=150 {
        delegate.remember_window(id);
    }
    assert_eq!(saved(), [2, 3, 1]);
    delegate.will_terminate(sel!(applicationWillTerminate:), &quit);
    assert_eq!(saved(), (23..=150).rev().collect::<Vec<_>>());
    drop(delegate);
    let large_id = (1u64 << 63) - 1;
    for (json, expected) in [
        ("[3,2,3,1,2]".into(), vec![3, 2, 1]),
        (
            serde_json::to_string(&(1..=150u64).collect::<Vec<_>>()).unwrap(),
            (1..=128).collect(),
        ),
        (format!("[{large_id},2]"), vec![large_id, 2]),
        ("not json".into(), vec![]),
        ("[1,-2]".into(), vec![]),
        ("[]".into(), vec![]),
    ] {
        // SAFETY: Test preferences use a disposable suite, never the user's domain.
        unsafe {
            defaults.setObject_forKey(Some(&NSString::from_str(&json)), key);
        }
        let delegate = responsive_fixture(mtm);
        delegate.restore_recency(defaults.clone());
        assert_eq!(*delegate.ivars().recency.borrow(), expected);
        delegate.remember_window(7);
        assert_eq!(defaults.stringForKey(key).unwrap().to_string(), json);
        delegate.will_terminate(sel!(applicationWillTerminate:), &quit);
        assert_eq!(saved()[0], 7);
    }
    defaults.removePersistentDomainForName(&domain);
    println!(
        "Recent-window history checks passed: save only on exit, restart, foreground capture, asynchronous correction, missing windows, bounded storage and corrupt data."
    );
}
