use core_foundation::base::TCFType;
use core_foundation::bundle::CFBundle;
use core_foundation::string::{CFString, CFStringRef};
use core_foundation::url::CFURL;
use objc2_foundation::MainThreadMarker;
use std::ffi::c_void;
use winlane::commands::CommandId;
use winlane::{tr, trf};

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPMFindPowerManagement(master_port: u32) -> u32;
    fn IOPMSleepSystem(connection: u32) -> i32;
    fn IOServiceClose(connection: u32) -> i32;
}

type LockScreen = unsafe extern "C" fn();
type DockNotification = unsafe extern "C" fn(CFStringRef, i32) -> i32;

enum Operation {
    Menu(crate::menu_bar::Reveal),
    Lock {
        _bundle: CFBundle,
        lock: LockScreen,
    },
    Sleep(PowerConnection),
    MissionControl {
        _bundle: CFBundle,
        send: DockNotification,
    },
}

pub struct PreparedCommand {
    operation: Operation,
    _main_thread: MainThreadMarker,
}

impl PreparedCommand {
    // Preparing may load a framework or open an IOKit connection, but never performs the action.
    pub fn prepare(
        command: CommandId,
        display: Option<u32>,
        mtm: MainThreadMarker,
    ) -> Result<Self, String> {
        let operation = match command {
            CommandId::Snippets => {
                return Err(tr!(
                    "请在搜索面板中打开片段搜索。",
                    "Open snippet search in the search panel."
                )
                .into());
            }
            CommandId::ShowMenu => {
                let display = display.ok_or_else(|| {
                    tr!(
                        "找不到当前屏幕，请重新打开搜索。",
                        "Display unavailable. Reopen search and try again."
                    )
                    .to_owned()
                })?;
                Operation::Menu(crate::menu_bar::Reveal::prepare(display, mtm)?)
            }
            CommandId::LockScreen => {
                let (bundle, function) = load_function(
                    "/System/Library/PrivateFrameworks/login.framework",
                    "SACLockScreenImmediate",
                )
                .ok_or_else(|| unavailable(command))?;
                Operation::Lock {
                    _bundle: bundle,
                    // SAFETY: login.framework exports SACLockScreenImmediate as void(void).
                    // Retaining the bundle keeps the resolved function loaded until execution ends.
                    lock: unsafe { std::mem::transmute::<*const c_void, LockScreen>(function) },
                }
            }
            CommandId::Sleep => {
                // SAFETY: MACH_PORT_NULL (0) selects the default power-management service.
                let connection = unsafe { IOPMFindPowerManagement(0) };
                if connection == 0 {
                    return Err(unavailable(command));
                }
                Operation::Sleep(PowerConnection(connection))
            }
            CommandId::MissionControl => {
                let (bundle, function) = load_function(
                    "/System/Library/Frameworks/ApplicationServices.framework/Frameworks/HIServices.framework",
                    "CoreDockSendNotification",
                ).ok_or_else(|| unavailable(command))?;
                Operation::MissionControl {
                    _bundle: bundle,
                    // SAFETY: The resolved CoreDock function has this signature. Retain its bundle.
                    send: unsafe {
                        std::mem::transmute::<*const c_void, DockNotification>(function)
                    },
                }
            }
        };
        Ok(Self {
            operation,
            _main_thread: mtm,
        })
    }

    pub fn execute(self) -> Result<(), String> {
        match self.operation {
            Operation::Menu(reveal) => reveal.show(),
            Operation::Lock { _bundle, lock } => {
                // SAFETY: The function was resolved from login.framework and the bundle is live.
                // This API has no result; issuing the request is not proof that the screen locked.
                unsafe { lock() };
            }
            Operation::Sleep(connection) => {
                // SAFETY: prepare opened this connection; its owner closes it on every exit path.
                check_status(CommandId::Sleep, unsafe { IOPMSleepSystem(connection.0) })?;
            }
            Operation::MissionControl { _bundle, send } => {
                let message = CFString::new("com.apple.expose.awake");
                // SAFETY: Both the message and the framework remain live for this call.
                check_status(CommandId::MissionControl, unsafe {
                    send(message.as_concrete_TypeRef(), 0)
                })?;
            }
        }
        Ok(())
    }
}

struct PowerConnection(u32);

impl Drop for PowerConnection {
    fn drop(&mut self) {
        // SAFETY: This is the owned connection returned by IOPMFindPowerManagement.
        unsafe { IOServiceClose(self.0) };
    }
}

fn load_function(path: &str, symbol: &str) -> Option<(CFBundle, *const c_void)> {
    let bundle = CFBundle::new(CFURL::from_path(path, true)?)?;
    let function = bundle.function_pointer_for_name(CFString::new(symbol));
    (!function.is_null()).then_some((bundle, function))
}

fn unavailable(command: CommandId) -> String {
    trf!(
        "当前 macOS 无法使用“{}”。",
        "“{}” is unavailable on this version of macOS.",
        command.definition().title()
    )
}

fn check_status(command: CommandId, status: i32) -> Result<(), String> {
    if status == 0 {
        Ok(())
    } else {
        Err(trf!(
            "无法执行“{}”（系统错误 0x{:08x}）。",
            "Could not run “{}” (system error 0x{:08x}).",
            command.definition().title(),
            status as u32
        ))
    }
}
