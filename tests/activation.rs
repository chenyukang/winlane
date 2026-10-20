use std::time::Duration;
use winlane::core::activation::*;
fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
fn request() -> Activation {
    Activation::new(20, 7, Some(70), Some(10), 99)
}
fn observed(front: i32, focused: u64) -> Observation {
    Observation {
        front_pid: Some(front),
        active: front == 20,
        running: true,
        window: WindowState {
            focused_id: Some(focused),
            on_screen: Some(true),
            alive: Some(true),
            ..WindowState::default()
        },
    }
}
#[test]
fn an_allowed_request_with_unchanged_foreground_never_confirms() {
    let mut r = request();
    assert_eq!(r.decide(ms(50), observed(10, 7), false), Decision::Wait);
    assert_eq!(r.decide(ms(250), observed(10, 7), false), Decision::Reopen);
    assert_eq!(
        r.decide(ms(1500), observed(10, 7), true),
        Decision::Failed(Failure::TimedOut)
    );
}
#[test]
fn confirmation_requires_two_settled_observations_of_the_selected_window() {
    let mut r = request();
    assert_eq!(r.decide(ms(20), observed(20, 7), false), Decision::Wait);
    assert_eq!(r.decide(ms(45), observed(20, 7), false), Decision::Wait);
    assert_eq!(
        r.decide(ms(70), observed(20, 7), false),
        Decision::Confirmed
    );
    assert_eq!(r.stable_ms(ms(70)), 50);
    let mut r = request();
    assert_eq!(r.decide(ms(20), observed(20, 8), false), Decision::Raise);
    assert_eq!(r.decide(ms(60), observed(20, 8), false), Decision::Wait);
}
#[test]
fn hidden_surface_or_inactive_target_does_not_pass_confirmation() {
    for field in ["surface", "active", "missing"] {
        let mut r = request();
        let mut o = observed(20, 7);
        match field {
            "surface" => o.window.on_screen = Some(false),
            "active" => o.active = false,
            _ => o.window.focused_id = None,
        }
        assert_ne!(r.decide(ms(20), o, false), Decision::Confirmed);
        assert_ne!(r.decide(ms(100), o, false), Decision::Confirmed);
    }
}
#[test]
fn a_rebuilt_ax_identity_can_match_the_same_server_window() {
    let mut r = request();
    let mut o = observed(20, 999);
    o.window.focused_server_id = Some(70);
    assert_eq!(r.decide(ms(20), o, false), Decision::Wait);
    assert_eq!(r.decide(ms(70), o, false), Decision::Confirmed);
}
#[test]
fn fallback_and_raises_are_bounded_and_require_foreground_first() {
    let mut r = request();
    assert_eq!(r.decide(ms(0), observed(20, 8), false), Decision::Raise);
    assert_eq!(r.decide(ms(150), observed(20, 8), false), Decision::Raise);
    assert_eq!(r.decide(ms(250), observed(20, 8), false), Decision::Reopen);
    assert_eq!(r.decide(ms(300), observed(20, 8), false), Decision::Wait);
    r.fallback_complete = true;
    assert_eq!(r.decide(ms(310), observed(20, 8), false), Decision::Raise);
    assert_eq!(r.decide(ms(460), observed(20, 8), false), Decision::Raise);
    assert_eq!(r.decide(ms(800), observed(20, 8), false), Decision::Wait);
    assert_eq!(
        r.decide(ms(1500), observed(20, 8), false),
        Decision::Failed(Failure::TimedOut)
    );
}
#[test]
fn third_app_or_return_to_origin_after_handoff_cancels() {
    let mut r = request();
    assert_eq!(
        r.decide(ms(10), observed(30, 7), false),
        Decision::Cancelled
    );
    let mut r = request();
    assert_eq!(r.decide(ms(10), observed(20, 7), false), Decision::Wait);
    assert_eq!(
        r.decide(ms(20), observed(10, 7), false),
        Decision::Cancelled
    );
}
#[test]
fn pending_writes_or_fallback_callbacks_cannot_confirm_and_stability_resets() {
    let mut r = request();
    assert_eq!(r.decide(ms(0), observed(20, 7), false), Decision::Wait);
    assert_eq!(r.decide(ms(20), observed(20, 8), false), Decision::Raise);
    assert_eq!(r.decide(ms(100), observed(20, 7), true), Decision::Wait);
    assert_eq!(r.decide(ms(120), observed(20, 7), false), Decision::Wait);
    assert_eq!(
        r.decide(ms(170), observed(20, 7), false),
        Decision::Confirmed
    );
}
#[test]
fn rejected_request_retries_but_exit_closed_window_and_deadline_are_terminal() {
    let mut r = request();
    r.request_allowed = false;
    assert_eq!(r.decide(ms(0), observed(10, 7), false), Decision::Reopen);
    let mut r = request();
    let mut o = observed(10, 7);
    o.running = false;
    assert_eq!(
        r.decide(ms(10), o, true),
        Decision::Failed(Failure::AppExited)
    );
    let mut r = request();
    let mut o = observed(10, 7);
    o.window.alive = Some(false);
    assert_eq!(
        r.decide(ms(10), o, false),
        Decision::Failed(Failure::WindowClosed)
    );
}
