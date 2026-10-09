use winlane::features::sleep_blockers::{Blocker, parse};

/// Trimmed from a real `pmset -g assertions` run while WeChat was holding a
/// video wake lock and the display was keeping the system awake.
const REAL_OUTPUT: &str = r#"2026-10-09 07:56:52 +0800 
Assertion status system-wide:
   BackgroundTask                 0
   ApplePushServiceTask           0
   UserIsActive                   1
   PreventUserIdleDisplaySleep    1
   SoftwareUpdateTask             0
   PreventSystemSleep             0
   PreventSystemSleepSecurityIndicator 0
   ExternalMedia                  0
   InternalPreventDisplaySleep    1
   PreventUserIdleSystemSleep     1
   NetworkClientActive            0
Listed by owning process:
   pid 41279(WeChatAppEx): [0x00044f0c00059e7e] 00:00:12 NoDisplaySleepAssertion named: "Video Wake Lock"  
   pid 610(bluetoothd): [0x00044f1700019e80] 00:00:01 PreventUserIdleSystemSleep named: "com.apple.BTStack"  
   pid 617(WindowServer): [0x00044e7c00099e64] 00:00:00 UserIsActive named: "com.apple.iohideventsystem.queue.tickle serviceID:100028467 service:AppleUserHIDEventService product:ERGO M575 eventType:17"  
	Timeout will fire in 300 secs Action=TimeoutActionRelease
   pid 561(powerd): [0x0003938d00018db7] 13:20:11 PreventUserIdleSystemSleep named: "Powerd - Prevent sleep while display is on"  
   pid 561(powerd): [0x0003dfe000108733] 00:00:12 InternalPreventDisplaySleep named: "com.apple.powermanagement.delayDisplayOff"  
	Timeout will fire in 288 secs Action=TimeoutActionTurnOff
Kernel Assertions: 0x100=MAGICWAKE
   id=561  level=255 0x100=MAGICWAKE creat=2026-09-30, 16:46  mod=2026-10-06, 23:12 description=en0 owner=IOSkywalkNetworkBSDClient
"#;

#[test]
fn reads_every_assertion_with_its_process_kind_and_held_time() {
    let blockers = parse(REAL_OUTPUT);
    assert_eq!(blockers.len(), 5, "{blockers:#?}");
    assert_eq!(
        blockers[0],
        Blocker {
            pid: 41279,
            name: "WeChatAppEx".into(),
            kind: "NoDisplaySleepAssertion".into(),
            reason: "Video Wake Lock".into(),
            held_secs: 12,
        }
    );
    assert_eq!(blockers[1].name, "bluetoothd");
    assert_eq!(blockers[1].kind, "PreventUserIdleSystemSleep");
    assert_eq!(
        blockers[2].reason,
        "com.apple.iohideventsystem.queue.tickle serviceID:100028467 service:AppleUserHIDEventService product:ERGO M575 eventType:17",
        "a reason with spaces and colons keeps its text"
    );
    assert_eq!(blockers[3].name, "powerd");
    assert_eq!(
        blockers[3].held_secs,
        13 * 3600 + 20 * 60 + 11,
        "hours are not truncated"
    );
    assert_eq!(blockers[4].kind, "InternalPreventDisplaySleep");
}

#[test]
fn only_assertions_that_hold_sleep_back_are_reported_as_blockers() {
    let blockers = parse(REAL_OUTPUT);
    let prevents: Vec<_> = blockers
        .iter()
        .filter(|blocker| blocker.prevents_sleep())
        .map(|blocker| (blocker.name.as_str(), blocker.kind.as_str()))
        .collect();
    assert_eq!(
        prevents,
        vec![
            ("WeChatAppEx", "NoDisplaySleepAssertion"),
            ("bluetoothd", "PreventUserIdleSystemSleep"),
            ("powerd", "PreventUserIdleSystemSleep"),
            ("powerd", "InternalPreventDisplaySleep"),
        ],
        "input activity is left out, display and idle assertions are not"
    );
    assert!(
        !blockers[2].prevents_sleep(),
        "UserIsActive is not a blocker"
    );
    assert_eq!(blockers[0].label(), "WeChatAppEx (Video Wake Lock)");
}

#[test]
fn a_process_without_a_reason_is_labelled_by_name() {
    let output = "Listed by owning process:\n   pid 900(caffeinate): [0x0001] 00:05:00 PreventUserIdleDisplaySleep\n";
    let blockers = parse(output);
    assert_eq!(blockers.len(), 1);
    assert_eq!(blockers[0].label(), "caffeinate");
    assert_eq!(blockers[0].held_secs, 300);
    assert!(blockers[0].reason.is_empty());
}

#[test]
fn unreadable_or_unrelated_input_yields_nothing_instead_of_guesses() {
    for input in [
        "",
        "Assertion status system-wide:\n   UserIsActive 1\n",
        "Could not read assertions\n",
        // Summary lines from the power log, not from `pmset -g assertions`.
        "2026-10-09 03:00:22 +0800 Assertions PID 561(powerd) Summary PreventUserIdleSystemSleep \"Powerd\" 08:23:41\n",
        // Half a line, and a duration that is not a duration.
        "   pid 42(foo): [0x1] 00:00 NoDisplaySleepAssertion named: \"x\"\n",
        "   pid notanumber(foo): [0x1] 00:01:00 NoDisplaySleepAssertion\n",
    ] {
        assert!(parse(input).is_empty(), "{input:?} must not parse");
    }
}
