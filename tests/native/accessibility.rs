use super::*;

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
