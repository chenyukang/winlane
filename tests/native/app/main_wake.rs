use super::*;

pub(super) fn verify_main_wake(mtm: MainThreadMarker) {
    use core_foundation::runloop::{CFRunLoop, kCFRunLoopDefaultMode};
    let received = Rc::new(RefCell::new(Vec::new()));
    let captured = received.clone();
    let calls = Rc::new(Cell::new(0));
    let callback_calls = calls.clone();
    let (tx, rx) = mpsc::channel();
    let wake = MainWake::new(mtm, move || {
        assert!(MainThreadMarker::new().is_some());
        callback_calls.set(callback_calls.get() + 1);
        captured.borrow_mut().extend(rx.try_iter());
    });
    let handle = wake.handle();
    let worker_handle = handle.clone();
    std::thread::spawn(move || {
        for value in 0..100 {
            tx.send(value).unwrap();
            worker_handle.signal();
        }
    })
    .join()
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while received.borrow().len() < 100 && Instant::now() < deadline {
        CFRunLoop::run_in_mode(
            unsafe { kCFRunLoopDefaultMode },
            Duration::from_millis(10),
            true,
        );
    }
    assert_eq!(*received.borrow(), (0..100).collect::<Vec<_>>());
    let before_drop = calls.get();
    drop(wake);
    handle.signal();
    CFRunLoop::run_in_mode(
        unsafe { kCFRunLoopDefaultMode },
        Duration::from_millis(5),
        false,
    );
    assert_eq!(received.borrow().len(), 100);
    assert_eq!(calls.get(), before_drop);
}
