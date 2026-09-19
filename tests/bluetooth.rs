use std::cell::Cell;
use winlane::core::commands::{CommandId, matching_commands};
use winlane::features::bluetooth::{Connection, Device, apply_connection, matching};

#[test]
fn explicit_bluetooth_entry_is_searchable_in_both_languages() {
    for query in ["bluetooth", " BLUETOOTH ", "blue", "bt", "蓝牙", "蓝牙设备"] {
        assert_eq!(matching_commands(query), [CommandId::Bluetooth]);
    }
    assert!(!matching_commands("b").contains(&CommandId::Bluetooth));
    assert_eq!(
        serde_json::to_string(&CommandId::Bluetooth).unwrap(),
        "\"bluetooth\""
    );
}

fn device(address: &str, name: &str, connected: bool) -> Device {
    Device {
        address: address.into(),
        name: name.into(),
        connected,
    }
}

#[test]
fn device_search_supports_words_unicode_and_duplicate_names() {
    let devices = vec![
        device("00-00-00-00-00-01", "Travel 耳机", false),
        device("00-00-00-00-00-02", "Keyboard", true),
        device("00-00-00-00-00-03", "Keyboard", true),
    ];
    assert_eq!(
        matching(&devices, "")
            .iter()
            .map(|d| &d.address)
            .collect::<Vec<_>>(),
        [
            &devices[1].address,
            &devices[2].address,
            &devices[0].address
        ]
    );
    assert_eq!(matching(&devices, " 耳机 TRAVEL "), [devices[0].clone()]);
    assert_eq!(matching(&devices, "keyboard 03"), [devices[2].clone()]);
    assert!(matching(&devices, "missing").is_empty());
}

struct FakeConnection {
    connected: Cell<bool>,
    calls: Cell<usize>,
    change_state: bool,
    fail: bool,
}

impl Connection for FakeConnection {
    fn connected(&self) -> bool {
        self.connected.get()
    }
    fn set_connected(&self, connected: bool) -> Result<(), String> {
        self.calls.set(self.calls.get() + 1);
        if self.change_state {
            self.connected.set(connected);
        }
        if self.fail {
            Err("device unavailable".into())
        } else {
            Ok(())
        }
    }
}

#[test]
fn both_operations_are_idempotent_when_cached_state_is_outdated() {
    for desired in [false, true] {
        let connection = FakeConnection {
            connected: Cell::new(desired),
            calls: Cell::new(0),
            change_state: false,
            fail: true,
        };
        assert!(apply_connection(&connection, desired).is_ok());
        assert_eq!(connection.calls.get(), 0);
        assert_eq!(connection.connected(), desired);
    }
}

#[test]
fn actual_connection_state_determines_success() {
    for desired in [false, true] {
        for change_state in [false, true] {
            for fail in [false, true] {
                let connection = FakeConnection {
                    connected: Cell::new(!desired),
                    calls: Cell::new(0),
                    change_state,
                    fail,
                };
                let result = apply_connection(&connection, desired);
                assert_eq!(result.is_ok(), change_state);
                assert_eq!(connection.calls.get(), 1);
                if fail && !change_state {
                    assert_eq!(result.unwrap_err(), "device unavailable");
                }
            }
        }
    }
}
