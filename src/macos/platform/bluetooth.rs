use objc2::msg_send;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2_foundation::{MainThreadMarker, NSArray, NSDictionary, NSNumber, NSString};
use std::time::{Duration, Instant};
use winlane::features::bluetooth::{Connection, Device, Update, apply_connection};
use winlane::{tr, trf};

#[link(name = "IOBluetooth", kind = "framework")]
unsafe extern "C" {}

#[link(name = "CoreBluetooth", kind = "framework")]
unsafe extern "C" {
    static CBCentralManagerOptionShowPowerAlertKey: &'static NSString;
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Authorization {
    Pending,
    Allowed,
    Denied,
}

pub(crate) fn authorization() -> Authorization {
    // SAFETY: CBManager's class property returns a CBManagerAuthorization (NSInteger).
    let value: isize = unsafe { msg_send![AnyClass::get(c"CBManager").unwrap(), authorization] };
    match value {
        0 => Authorization::Pending,
        3 => Authorization::Allowed,
        _ => Authorization::Denied,
    }
}

pub(crate) fn request_access(_: MainThreadMarker) -> Retained<AnyObject> {
    // Retain the manager until the user answers the system permission prompt.
    // No scanning, pairing, or power changes are requested.
    unsafe {
        let options = NSDictionary::from_slices(
            &[CBCentralManagerOptionShowPowerAlertKey],
            &[&*NSNumber::new_bool(false)],
        );
        let manager: objc2::rc::Allocated<AnyObject> =
            msg_send![AnyClass::get(c"CBCentralManager").unwrap(), alloc];
        msg_send![manager, initWithDelegate: std::ptr::null::<AnyObject>(),
            queue: std::ptr::null::<AnyObject>(), options: &*options]
    }
}

pub(crate) fn permission_error() -> String {
    tr!(
        "请在系统设置 → 隐私与安全性 → 蓝牙中允许 Winlane，然后按 ⌘R 重试。",
        "Allow Winlane in System Settings → Privacy & Security → Bluetooth, then press ⌘R to retry."
    )
    .into()
}

fn check_access() -> Result<(), String> {
    if authorization() != Authorization::Allowed {
        return Err(permission_error());
    }
    // SAFETY: These are public IOBluetoothHostController selectors. The SDK
    // declares powerState as BluetoothHCIPowerState (a 32-bit enum).
    let controller: Option<Retained<AnyObject>> = unsafe {
        msg_send![
            AnyClass::get(c"IOBluetoothHostController").unwrap(),
            defaultController
        ]
    };
    let Some(controller) = controller else {
        return Err(tr!("蓝牙不可用。", "Bluetooth is unavailable.").into());
    };
    let power: u32 = unsafe { msg_send![&*controller, powerState] };
    if power != 1 {
        return Err(tr!(
            "蓝牙已关闭，请在系统设置中打开蓝牙，再按 ⌘R 刷新。",
            "Bluetooth is off. Turn it on in System Settings, then press ⌘R to refresh."
        )
        .into());
    }
    Ok(())
}

fn devices() -> Result<Vec<Device>, String> {
    check_access()?;
    // SAFETY: pairedDevices returns an autoreleased NSArray of IOBluetoothDevice
    // objects. Retained keeps the array and its elements alive for this worker.
    let devices: Option<Retained<NSArray<AnyObject>>> =
        unsafe { msg_send![AnyClass::get(c"IOBluetoothDevice").unwrap(), pairedDevices] };
    let mut result = Vec::new();
    if let Some(devices) = devices {
        for device in devices.iter() {
            let address: Option<Retained<NSString>> = unsafe { msg_send![&*device, addressString] };
            let name: Option<Retained<NSString>> = unsafe { msg_send![&*device, nameOrAddress] };
            // BluetoothAssignedNumbers.h identifies headsets, speakers and stereos
            // with major device class 0x04; names may be user-customized.
            let device_class: u32 = unsafe { msg_send![&*device, deviceClassMajor] };
            if let Some(address) = address {
                let address = address.to_string();
                result.push(Device {
                    name: name.map_or_else(|| address.clone(), |name| name.to_string()),
                    address,
                    connected: NativeConnection(device).connected(),
                    is_audio: device_class == 0x04,
                });
            }
        }
    }
    Ok(result)
}

pub(crate) fn load() -> Update {
    autoreleasepool(|_| match devices() {
        Ok(devices) => Update {
            devices: Some(devices),
            error: None,
        },
        Err(error) => Update {
            devices: None,
            error: Some(error),
        },
    })
}

pub(crate) fn set_connection(address: &str, connected: bool) -> Update {
    autoreleasepool(|_| {
        let operation = (|| {
            check_access()?;
            let address = NSString::from_str(address);
            let device: Option<Retained<AnyObject>> = unsafe {
                msg_send![AnyClass::get(c"IOBluetoothDevice").unwrap(), deviceWithAddressString: &*address]
            };
            let Some(device) = device else {
                return Err(tr!("找不到该蓝牙设备。", "Bluetooth device not found.").into());
            };
            let paired: Bool = unsafe { msg_send![&*device, isPaired] };
            if !paired.as_bool() {
                return Err(tr!(
                    "该设备已取消配对，请在系统设置中重新配对。",
                    "This device is no longer paired. Pair it in System Settings first."
                )
                .into());
            }
            apply_connection(&NativeConnection(device), connected)
        })();
        let mut update = load();
        if let Err(error) = operation {
            update.error = Some(error);
        }
        update
    })
}

struct NativeConnection(Retained<AnyObject>);

impl Connection for NativeConnection {
    fn connected(&self) -> bool {
        let connected: Bool = unsafe { msg_send![&*self.0, isConnected] };
        connected.as_bool()
    }

    fn set_connected(&self, connected: bool) -> Result<(), String> {
        // Both operations are synchronous and run only on the Bluetooth worker.
        // 16,000 Bluetooth slots give a ten-second connection page timeout.
        let status: i32 = unsafe {
            if connected {
                msg_send![&*self.0, openConnection: std::ptr::null::<AnyObject>(),
                    withPageTimeout: 16_000_u16, authenticationRequired: Bool::YES]
            } else {
                msg_send![&*self.0, closeConnection]
            }
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        while status == 0 && self.connected() != connected && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        if status == 0 {
            Ok(())
        } else {
            Err(trf!(
                "蓝牙操作失败（{}），请确认设备已开机且在附近，再按 ⌘R 重试。",
                "Bluetooth operation failed ({}). Check that the device is on and nearby, then press ⌘R to retry.",
                status
            ))
        }
    }
}
