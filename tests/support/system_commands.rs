use std::sync::atomic::{AtomicUsize, Ordering};

static LOCK_CALLS: AtomicUsize = AtomicUsize::new(0);
static MISSION_CONTROL_CALLS: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn record_mission_control(message: CFStringRef, flags: i32) -> i32 {
    if message.is_null() || flags != 0 { return 1001; }
    let message = unsafe { CFString::wrap_under_get_rule(message) };
    if message != "com.apple.expose.awake" { return 1001; }
    MISSION_CONTROL_CALLS.fetch_add(1, Ordering::Relaxed);
    0
}

unsafe extern "C" fn record_lock() {
    LOCK_CALLS.fetch_add(1, Ordering::Relaxed);
}

pub fn verify_prepared_commands(mtm: MainThreadMarker) {
    // Resolve actual system interfaces, but replace callable functions before exercising dispatch.
    let mut lock = PreparedCommand::prepare(CommandId::LockScreen, None, mtm).unwrap();
    let Operation::Lock { lock: function, .. } = &mut lock.operation else {
        panic!("lock-screen must resolve the lock operation");
    };
    *function = record_lock;
    assert_eq!(LOCK_CALLS.load(Ordering::Relaxed), 0);
    lock.execute().unwrap();
    assert_eq!(LOCK_CALLS.load(Ordering::Relaxed), 1);

    let sleep = PreparedCommand::prepare(CommandId::Sleep, None, mtm).unwrap();
    assert!(matches!(sleep.operation, Operation::Sleep(_)));
    drop(sleep); // Close the connection without requesting sleep.

    let mut mission_control = PreparedCommand::prepare(CommandId::MissionControl, None, mtm).unwrap();
    let Operation::MissionControl { send, .. } = &mut mission_control.operation else {
        panic!("mission-control must resolve the Mission Control operation");
    };
    *send = record_mission_control;
    assert_eq!(MISSION_CONTROL_CALLS.load(Ordering::Relaxed), 0);
    mission_control.execute().unwrap();
    assert_eq!(MISSION_CONTROL_CALLS.load(Ordering::Relaxed), 1);

    assert!(load_function("/System/Library/PrivateFrameworks/login.framework", "WinlaneMissingSymbol").is_none());
    assert!(load_function("/System/Library/Frameworks/WinlaneMissing.framework", "Missing").is_none());
    assert!(PreparedCommand::prepare(CommandId::ShowMenu, None, mtm).is_err());
    for command in [CommandId::Sleep, CommandId::MissionControl] {
        assert!(check_status(command, 0).is_ok());
        let error = check_status(command, -1).unwrap_err();
        assert!(error.contains(command.definition().title()));
        assert!(error.contains("ffffffff"));
    }
    println!("System command checks passed: native interfaces resolved, dispatch recorded, error handling verified; no screen locked, sleep requested or desktop changed.");
}
