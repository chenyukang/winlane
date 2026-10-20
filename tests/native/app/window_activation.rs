use super::*;
use crate::macos::app::window_activation::PendingWindowActivation;
use crate::macos::platform::logging;
use std::sync::atomic::Ordering;
use winlane::core::activation::{Decision, Failure, Observation, WindowState};

fn pending(delegate: &Delegate, id: u64) {
    let state = delegate.ivars();
    let window = WindowInfo {
        id: 7,
        pid: -20,
        app: "Activation fixture".into(),
        title: "sensitive-window-title-not-for-logs".into(),
        minimized: false,
    };
    let request = PendingWindowActivation::new(
        id,
        state.session.get(),
        window,
        "example.activation-test".into(),
        "sensitive-search-not-for-logs".into(),
        Some(-10),
        Some(70),
    );
    request.trace("requested", logging::Level::Info, serde_json::json!({}));
    state.window_activation.replace(Some(request));
    let timer = unsafe {
        NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
            60.0,
            delegate,
            sel!(verifyWindowActivation:),
            None,
            true,
        )
    };
    state.window_activation_timer.replace(Some(timer));
}
fn observed(front: i32) -> Observation {
    Observation {
        front_pid: Some(front),
        active: front == -20,
        running: true,
        window: WindowState {
            focused_id: Some(7),
            focused_server_id: Some(70),
            alive: Some(true),
            on_screen: Some(true),
            ax_error: None,
        },
    }
}

pub(super) fn verify(mtm: MainThreadMarker) {
    // An isolated destination lets the same production logger and external
    // verifier inspect genuine emitted records, without changing user settings.
    let temp = tempfile::tempdir().unwrap();
    let path = std::env::var("WINLANE_ACTIVATION_TEST_LOG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| temp.path().join("activation.log"));
    logging::init(&winlane::core::logging::Settings {
        level: logging::Level::Info,
        file_path: path.to_string_lossy().into_owned(),
    });
    let delegate = responsive_fixture(mtm);
    let state = delegate.ivars();
    let before = state.recency.borrow().clone();
    pending(&delegate, 1001);
    delegate.observe_window_activation(1001, observed(-10));
    assert!(state.window_activation.borrow().is_some());
    assert_eq!(
        *state.recency.borrow(),
        before,
        "API acceptance is not a real visit"
    );
    {
        let mut slot = state.window_activation.borrow_mut();
        slot.as_mut().unwrap().started = Instant::now() - Duration::from_millis(60);
    }
    delegate.observe_window_activation(1001, observed(-20));
    assert!(state.window_activation.borrow().is_some());
    let timer = state
        .window_activation_timer
        .borrow()
        .as_ref()
        .unwrap()
        .clone();
    {
        let mut slot = state.window_activation.borrow_mut();
        slot.as_mut().unwrap().started = Instant::now() - Duration::from_millis(150);
    }
    delegate.observe_window_activation(1001, observed(-20));
    assert!(state.window_activation.borrow().is_none());
    assert!(!timer.isValid());
    assert_eq!(state.recency.borrow()[0], 7);
    assert_eq!(
        state
            .preferences
            .borrow()
            .get("sensitive-search-not-for-logs"),
        Some(&7)
    );

    pending(&delegate, 1002);
    let token = state
        .window_activation
        .borrow()
        .as_ref()
        .unwrap()
        .current
        .clone();
    let timer = state
        .window_activation_timer
        .borrow()
        .as_ref()
        .unwrap()
        .clone();
    delegate.prepare_panel(PanelMode::Switch, 123, 1);
    assert!(
        !token.load(Ordering::Acquire),
        "new gestures revoke in-flight AX write permissions"
    );
    assert!(!timer.isValid());
    assert!(state.window_activation.borrow().is_none());
    delegate.end_session();
    pending(&delegate, 1003);
    delegate.finish_window_activation(1002, Decision::Cancelled, observed(-10));
    delegate.finish_window_activation(1002, Decision::Confirmed, observed(-20));
    assert_eq!(
        state.window_activation.borrow().as_ref().unwrap().id,
        1003,
        "stale terminal callbacks cannot cancel or commit a newer handoff"
    );
    let before = state.recency.borrow().clone();
    delegate.finish_window_activation(1003, Decision::Failed(Failure::TimedOut), observed(-10));
    assert!(state.window_activation.borrow().is_none());
    assert!(state.window_activation_timer.borrow().is_none());
    assert_eq!(*state.recency.borrow(), before);

    let operations = std::cell::RefCell::new(Vec::new());
    assert!(
        accessibility::raise_window_guarded(-1, 7, None, &|| false, &|op, status, code| operations
            .borrow_mut()
            .push((op.to_string(), status.to_string(), code)))
        .is_err()
    );
    assert_eq!(
        *operations.borrow(),
        [("lookup".to_string(), "cancelled".to_string(), None)],
        "cancelled work must stop before even looking up an AX target"
    );
    logging::flush();
    let contents = std::fs::read_to_string(&path).unwrap();
    assert!(contents.contains("event=\"confirmed\""));
    assert!(contents.contains("event=\"failed\""));
    assert!(contents.contains("event=\"cancelled\""));
    assert!(!contents.contains("sensitive-window-title"));
    assert!(!contents.contains("sensitive-search"));
    println!(
        "Window activation verification checks passed: allowed-but-not-front never commits, stable exact focus confirms, new gestures revoke writes, stale completions cannot affect new requests, timeout stops the timer; no apps activated. Isolated log: {}",
        path.display()
    );
}
