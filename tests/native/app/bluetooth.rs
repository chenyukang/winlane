use super::*;
use winlane::features::bluetooth::{Device, Update};

pub(super) fn verify_bluetooth(mtm: MainThreadMarker) {
    verify_native_api();
    let devices = vec![
        Device {
            address: "00-00-00-00-00-01".into(),
            name: "Keyboard".into(),
            connected: true,
        },
        Device {
            address: "00-00-00-00-00-02".into(),
            name: "Headphones 耳机".into(),
            connected: false,
        },
    ];
    for cached in [false, true] {
        let delegate = responsive_fixture(mtm);
        let state = delegate.ivars();
        if cached {
            state.bluetooth_devices.replace(devices.clone());
        }
        delegate.prepare_command_search(CommandId::Bluetooth, 101);
        assert!(delegate.searching_bluetooth());
        assert_eq!(delegate.match_count(), if cached { 2 } else { 0 });
        assert!(state.bluetooth_receiver.borrow().is_none());
        assert!(state.bluetooth_permission.borrow().is_none());
        assert!(state.scoped_refresh_timer.borrow().is_some());
        assert!(
            delegate
                .panels()
                .iter()
                .all(|ui| !ui.project_progress.isHidden())
        );
        delegate.end_session();
    }

    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.catalog_checked.set(Some(Instant::now()));
    state.mode.set(Some(PanelMode::Search));
    delegate.sync_displays();
    state.query.replace("bluetooth".into());
    delegate.filter();
    assert_eq!(delegate.selected_command(), Some(CommandId::Bluetooth));
    assert!(state.bluetooth_receiver.borrow().is_none());
    assert!(state.bluetooth_permission.borrow().is_none());
    let (tx, rx) = mpsc::channel();
    state.bluetooth_receiver.replace(Some(rx));
    delegate.activate_selected();
    assert!(delegate.searching_bluetooth());
    state.query.replace("耳机".into());
    delegate.filter();
    delegate.activate_selected(); // A load is pending; this must not touch real devices.
    tx.send(Update {
        devices: Some(devices.clone()),
        error: None,
    })
    .unwrap();
    delegate.poll_bluetooth();
    assert_eq!(delegate.match_count(), 1);
    assert_eq!(delegate.selected_bluetooth(), Some(devices[1].clone()));
    assert!(delegate.panels().iter().all(|ui| {
        let rows = ui.rows.borrow();
        rows[0].selected == Some(true)
            && rows[0].title.stringValue().to_string() == devices[1].name
            && ui.project_progress.isHidden()
    }));
    state.query.borrow_mut().clear();
    delegate.filter();
    delegate.move_selection(1);
    let (tx, rx) = mpsc::channel();
    state.bluetooth_receiver.replace(Some(rx));
    state
        .bluetooth_pending
        .replace(Some((devices[1].address.clone(), true)));
    delegate.render();
    delegate.activate_selected(); // Repeated Enter must not start a second worker.
    assert!(state.bluetooth_pending.borrow().as_ref().unwrap().1);
    assert!(delegate.panels().iter().all(|ui| matches!(
        &ui.rows.borrow()[1].content,
        Some(RowContent::Bluetooth(_, Some(true)))
    )));
    let mut updated = devices.clone();
    updated[1].connected = true;
    tx.send(Update {
        devices: Some(updated.clone()),
        error: None,
    })
    .unwrap();
    delegate.poll_bluetooth();
    assert_eq!(
        delegate.selected_bluetooth(),
        Some(updated[1].clone()),
        "selection follows device identity after connected-first reordering"
    );
    assert!(state.bluetooth_pending.borrow().is_none());
    assert_eq!(state.mode.get(), Some(PanelMode::Search));
    state.config.borrow_mut().show_usage_hints = false;
    let (tx, rx) = mpsc::channel();
    state.bluetooth_receiver.replace(Some(rx));
    tx.send(Update {
        devices: None,
        error: Some("Test Bluetooth failure".into()),
    })
    .unwrap();
    delegate.poll_bluetooth();
    assert_eq!(state.bluetooth_devices.borrow().len(), 2);
    assert!(delegate.panels().iter().all(|ui| !ui.footer.isHidden()
        && ui.footer.stringValue().to_string() == "Test Bluetooth failure"));

    let (tx, rx) = mpsc::channel();
    state.bluetooth_receiver.replace(Some(rx));
    delegate.leave_scoped_search();
    assert_eq!(delegate.selected_command(), Some(CommandId::Bluetooth));
    assert!(state.bluetooth_matches.borrow().is_empty());
    tx.send(Update {
        devices: Some(Vec::new()),
        error: None,
    })
    .unwrap();
    delegate.poll_bluetooth();
    assert!(!delegate.searching_bluetooth());
    assert_eq!(delegate.selected_command(), Some(CommandId::Bluetooth));
    assert!(state.bluetooth_devices.borrow().is_empty());

    delegate.enter_scoped_search(SearchScope::Bluetooth);
    delegate.cancel_scoped_refresh();
    let (tx, rx) = mpsc::channel();
    state.bluetooth_receiver.replace(Some(rx));
    drop(tx);
    delegate.poll_bluetooth();
    assert!(state.bluetooth_error.borrow().is_some());
    assert!(!delegate.bluetooth_busy());
    delegate.end_session();
    assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));
    println!(
        "Bluetooth: cached entry, async filtering, identity selection, pending operations, failure recovery and native API signatures; no real devices changed."
    );
}

fn verify_native_api() {
    use objc2::runtime::AnyClass;
    let device = AnyClass::get(c"IOBluetoothDevice").expect("IOBluetooth is linked");
    for selector in [sel!(pairedDevices), sel!(deviceWithAddressString:)] {
        assert!(device.class_method(selector).is_some());
    }
    for selector in [
        sel!(nameOrAddress),
        sel!(addressString),
        sel!(isPaired),
        sel!(isConnected),
        sel!(closeConnection),
        sel!(openConnection:withPageTimeout:authenticationRequired:),
    ] {
        assert!(device.instance_method(selector).is_some());
    }
    let connect = device
        .instance_method(sel!(openConnection:withPageTimeout:authenticationRequired:))
        .unwrap();
    assert_eq!(connect.return_type().to_str().unwrap(), "i");
    assert_eq!(connect.argument_type(2).unwrap().to_str().unwrap(), "@");
    assert_eq!(connect.argument_type(3).unwrap().to_str().unwrap(), "S");
    let manager = AnyClass::get(c"CBCentralManager")
        .unwrap()
        .instance_method(sel!(initWithDelegate:queue:options:))
        .unwrap();
    assert_eq!(manager.argument_type(3).unwrap().to_str().unwrap(), "@");
    let controller = AnyClass::get(c"IOBluetoothHostController").unwrap();
    assert!(controller.class_method(sel!(defaultController)).is_some());
    assert!(controller.instance_method(sel!(powerState)).is_some());
    assert!(
        AnyClass::get(c"CBManager")
            .unwrap()
            .class_method(sel!(authorization))
            .is_some()
    );
    assert!(
        AnyClass::get(c"CBCentralManager")
            .unwrap()
            .instance_method(sel!(initWithDelegate:queue:options:))
            .is_some()
    );
}
