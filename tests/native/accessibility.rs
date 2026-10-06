use super::*;

pub fn inspect_published_window_changes(pid: i32) {
    let mut inventory = Inventory::read();
    let application = Element::application(pid).unwrap();
    let windows = all_windows(&application, pid, &inventory, true);
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
        let found = complete_windows(vec![Element(published.0.clone())], pid, &inventory, true);
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

    let found = complete_windows(Vec::new(), pid, &inventory, true);
    let found: HashSet<_> = found.iter().filter_map(Element::server_id).collect();
    assert_eq!(
        found, expected,
        "an empty published list must recover live windows"
    );

    let closed = windows.last().unwrap().server_id().unwrap();
    inventory.normal.get_mut(&pid).unwrap().remove(&closed);
    let found = complete_windows(vec![Element(windows[0].0.clone())], pid, &inventory, true);
    let found: HashSet<_> = found.iter().filter_map(Element::server_id).collect();
    assert!(
        !found.contains(&closed),
        "closed windows must leave the cache"
    );
    assert_eq!(found.len(), expected.len() - 1);
    inventory.normal.get_mut(&pid).unwrap().insert(closed);
    let found = complete_windows(vec![Element(windows[0].0.clone())], pid, &inventory, true);
    let found: HashSet<_> = found.iter().filter_map(Element::server_id).collect();
    assert_eq!(
        found, expected,
        "a new inventory entry must restart discovery"
    );
    println!("Empty published list, closed window removal, and inventory addition passed.");
    remote_scans().lock().unwrap().remove(&pid);
}

/// Windows seen in an earlier scan must stay listed while their WindowServer
/// surfaces are still alive, even when accessibility queries stop publishing
/// them (as happens after minimizing).
pub fn inspect_remembered_windows_stay_listed(pid: i32) {
    let mut inventory = Inventory::read();
    let application = Element::application(pid).unwrap();
    let live = all_windows(&application, pid, &inventory, true);
    let element = Element(live.first().expect("requires a live window").0.clone());
    let info = WindowInfo {
        id: element.id(pid),
        pid,
        app: "Winlane".into(),
        title: "remembered".into(),
        minimized: true,
    };
    remote_scans().lock().unwrap().remove(&pid);

    // Simulate a minimized window: its server id stays in the inventory but
    // no accessibility query publishes an element for it.
    let fake_server_id = u32::MAX;
    inventory
        .normal
        .entry(pid)
        .or_default()
        .insert(fake_server_id);
    remember_window(pid, fake_server_id, &info, &element);

    let scanned = scan_application(pid, "Winlane", &inventory, true).unwrap();
    let remembered = scanned
        .iter()
        .find(|(window, _)| window.id == info.id)
        .expect("remembered window must stay listed");
    assert_eq!(remembered.1, Some(fake_server_id));

    // Once the surface leaves the inventory the window is gone for good.
    inventory
        .normal
        .get_mut(&pid)
        .unwrap()
        .remove(&fake_server_id);
    let scanned = scan_application(pid, "Winlane", &inventory, true).unwrap();
    assert!(
        scanned.iter().all(|(window, _)| window.id != info.id),
        "closed windows must leave the list"
    );

    // With minimized-window tracking disabled, remembered data is released
    // and hidden windows are not merged back into the list.
    inventory
        .normal
        .entry(pid)
        .or_default()
        .insert(fake_server_id);
    remember_window(pid, fake_server_id, &info, &element);
    let scanned = scan_application(pid, "Winlane", &inventory, false).unwrap();
    assert!(
        scanned.iter().all(|(window, _)| window.id != info.id),
        "tracking disabled must not list remembered windows"
    );
    assert!(
        remembered_windows()
            .lock()
            .unwrap()
            .get(&pid)
            .is_none_or(|entries| entries.is_empty()),
        "tracking disabled must drop remembered windows"
    );
    inventory
        .normal
        .get_mut(&pid)
        .unwrap()
        .remove(&fake_server_id);

    remote_scans().lock().unwrap().remove(&pid);
    remembered_windows().lock().unwrap().remove(&pid);
    println!("Remembered windows stay listed while alive and leave when closed.");
}

/// An app can rebuild its accessibility objects, so the remembered AX identity
/// stops matching while the window itself is still there. Finding that window
/// again must accept the WindowServer identity, must never focus a different
/// window, and must report a closed window instead of guessing.
pub fn verify_fresh_window_lookup() {
    // The AX identity still matches.
    assert_eq!(
        matching_window(&[(7, Some(1)), (9, Some(2))], 9, Some(2)),
        Ok(1)
    );
    // Only the WindowServer identity survives the rebuild.
    assert_eq!(
        matching_window(&[(7, Some(1)), (8, Some(2))], 9, Some(2)),
        Ok(1)
    );
    // A window published twice is still one window.
    assert_eq!(
        matching_window(&[(9, Some(2)), (9, Some(2))], 9, Some(2)),
        Ok(0)
    );
    // Two different windows matching must not focus either one.
    assert_eq!(
        matching_window(&[(8, Some(2)), (9, Some(3))], 9, Some(2)),
        Err(WindowMatch::Ambiguous)
    );
    // The window closed, and an unknown handle has no identity to fall back on.
    assert_eq!(
        matching_window(&[(7, Some(1))], 9, Some(2)),
        Err(WindowMatch::Missing)
    );
    assert_eq!(matching_window(&[], 9, None), Err(WindowMatch::Missing));
    println!(
        "Fresh-window lookup passed: WindowServer-id recovery plus duplicate, ambiguous and closed windows."
    );
}

/// An app can rebuild its accessibility objects (WeChat's Chromium UI does),
/// which invalidates the handle a row was built from and changes the AX-derived
/// identity while the window itself stays. Selecting that row must find the
/// window again through its WindowServer identity instead of reporting it
/// closed, and must still refuse to guess when nothing matches.
pub fn verify_rebuilt_identity_lookup() {
    assert!(
        is_trusted(),
        "the rebuilt-identity check requires accessibility access"
    );
    let apps: Vec<(i32, String)> = objc2_app_kit::NSWorkspace::sharedWorkspace()
        .runningApplications()
        .iter()
        .filter(|app| {
            app.activationPolicy() == objc2_app_kit::NSApplicationActivationPolicy::Regular
        })
        .map(|app| {
            (
                app.processIdentifier(),
                app.localizedName().unwrap().to_string(),
            )
        })
        .collect();
    let (windows, server_ids) = list_windows(&apps, true);
    let Some(window) = windows
        .iter()
        .find(|window| server_ids.contains_key(&window.id))
    else {
        println!("Rebuilt-identity check skipped: no listed window has a WindowServer identity.");
        return;
    };
    let server_id = server_ids[&window.id];
    // Stale rows keep the AX identity of the accessibility object that was
    // rebuilt. Model that: the same window, a different AX identity, and the
    // handle the row was built from no longer answering reads.
    let rebuilt_id = window.id ^ (1 << 62);
    // macOS answers a bogus remote token with "invalid element" for some tokens
    // and "cannot complete" for others, depending on the app, so try a few and
    // only model the rebuild when a handle really reads as invalid.
    let dead = [u64::MAX, 7, 0x1092, 1u64 << 40, 4242]
        .into_iter()
        .filter_map(|token| Element::from_remote_id(window.pid, token))
        .find(|element| element.attribute_once("AXRole") == Err(AX_INVALID_UI_ELEMENT));
    let Some(dead) = dead else {
        println!(
            "Rebuilt-identity check skipped: {} never reports an invalid handle for a bogus token.",
            window.app
        );
        return;
    };
    remember_window(
        window.pid,
        server_id,
        &WindowInfo {
            id: rebuilt_id,
            ..window.clone()
        },
        &dead,
    );
    assert!(
        window_is_open(window.pid, rebuilt_id),
        "a rebuilt accessibility object must not look like a closed window"
    );
    assert!(
        !window_is_open(window.pid, rebuilt_id ^ (1 << 61)),
        "a window nothing matches must still be reported closed"
    );
    if let Some(entries) = remembered_windows().lock().unwrap().get_mut(&window.pid) {
        entries.remove(&server_id);
    }
    remote_scans().lock().unwrap().remove(&window.pid);
    println!("Rebuilt-identity lookup passed: WindowServer identity recovers the window.");
}
