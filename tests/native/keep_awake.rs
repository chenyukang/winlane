use super::*;
use core_foundation::{
    base::{CFRelease, CFTypeRef},
    dictionary::CFDictionaryRef,
};

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPMAssertionCopyProperties(id: u32) -> CFDictionaryRef;
}
fn exists(id: u32) -> bool {
    let properties = unsafe { IOPMAssertionCopyProperties(id) };
    if properties.is_null() {
        return false;
    }
    unsafe {
        CFRelease(properties as CFTypeRef);
    }
    true
}

pub fn verify() {
    let mut runtime = KeepAwake::default();
    assert!(runtime.indicator_status(SystemTime::now()).is_none());
    runtime
        .apply(Choice::Start {
            minutes: Some(30),
            display: false,
        })
        .unwrap();
    assert_eq!(
        runtime
            .indicator_status(SystemTime::now())
            .unwrap()
            .remaining_minutes,
        Some(30)
    );
    assert!(!runtime.indicator_status(SystemTime::now()).unwrap().display);
    let first = runtime.active.as_ref().unwrap()._assertions[0].0;
    assert!(exists(first));
    assert!(!runtime.expire(SystemTime::now()));
    runtime
        .apply(Choice::Start {
            minutes: None,
            display: true,
        })
        .unwrap();
    assert!(!exists(first));
    let status = runtime.indicator_status(SystemTime::now()).unwrap();
    assert!(status.display);
    assert!(status.remaining_minutes.is_none());
    let ids: Vec<_> = runtime
        .active
        .as_ref()
        .unwrap()
        ._assertions
        .iter()
        .map(|a| a.0)
        .collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.iter().all(|id| exists(*id)));
    assert!(!runtime.expire(SystemTime::now() + std::time::Duration::from_secs(86400)));
    runtime.apply(Choice::Stop).unwrap();
    assert!(runtime.indicator_status(SystemTime::now()).is_none());
    assert!(ids.iter().all(|id| !exists(*id)));
    runtime.apply(Choice::Stop).unwrap();
    runtime
        .apply(Choice::Start {
            minutes: Some(30),
            display: false,
        })
        .unwrap();
    let id = runtime.active.as_ref().unwrap()._assertions[0].0;
    let deadline = runtime.active.as_ref().unwrap().deadline.unwrap();
    assert_eq!(
        runtime.current_choice(SystemTime::now()),
        Choice::Start {
            minutes: Some(30),
            display: false
        }
    );
    assert_eq!(runtime.current_choice(deadline), Choice::Stop);
    assert!(
        runtime.active.is_some(),
        "selection must recognize expiry even before the timer clears it"
    );
    assert!(runtime.indicator_status(deadline).is_none());
    assert!(runtime.expire(deadline));
    assert!(!exists(id));
    // The system must also release a timed assertion without polling Winlane.
    let assertion = Assertion::new("PreventUserIdleSystemSleep", 0.05).unwrap();
    let start = std::time::Instant::now();
    while exists(assertion.0) && start.elapsed() < std::time::Duration::from_secs(3) {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(!exists(assertion.0));
    let assertion = Assertion::new("PreventUserIdleSystemSleep", 0.0).unwrap();
    let id = assertion.0;
    drop(assertion);
    assert!(!exists(id));
}
