pub(crate) fn escape_event(window: &NSWindow, flags: NSEventModifierFlags) -> Retained<NSEvent> {
    NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
        NSEventType::KeyDown,
        NSPoint::new(0.0, 0.0),
        flags,
        0.0,
        window.windowNumber(),
        None,
        ns_string!("\u{1b}"),
        ns_string!("\u{1b}"),
        false,
        53,
    )
    .unwrap()
}

pub fn verify_escape_close(window: &NSWindow) {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let closed = Arc::new(AtomicUsize::new(0));
    let count = closed.clone();
    let block = block2::RcBlock::new(
        move |_: std::ptr::NonNull<objc2_foundation::NSNotification>| {
            count.fetch_add(1, Ordering::Relaxed);
        },
    );
    let center = objc2_foundation::NSNotificationCenter::defaultCenter();
    // SAFETY: The callback only touches an atomic and observes this retained window.
    let observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWindowWillCloseNotification),
            Some(window),
            None,
            &block,
        )
    };
    window.sendEvent(&escape_event(window, NSEventModifierFlags::Command));
    assert_eq!(
        closed.load(Ordering::Relaxed),
        0,
        "modified Escape must not close settings"
    );
    let mtm = MainThreadMarker::new().unwrap();
    let editor = NSTextView::initWithFrame(NSTextView::alloc(mtm), rect(0.0, 0.0, 100.0, 24.0));
    window.contentView().unwrap().addSubview(&editor);
    assert!(window.makeFirstResponder(Some(&editor)));
    // SAFETY: A native text input client accepts NSString and valid UTF-16 ranges.
    unsafe {
        NSTextInputClient::setMarkedText_selectedRange_replacementRange(
            &*editor,
            ns_string!("zhong"),
            objc2_foundation::NSRange::new(5, 0),
            objc2_foundation::NSRange::new(objc2_foundation::NSNotFound as usize, 0),
        );
    }
    assert!(NSTextInputClient::hasMarkedText(&*editor));
    window.sendEvent(&escape_event(window, NSEventModifierFlags::empty()));
    assert_eq!(
        closed.load(Ordering::Relaxed),
        0,
        "IME cancellation must not close settings"
    );
    NSTextInputClient::unmarkText(&*editor);
    assert!(window.makeFirstResponder(None));
    editor.removeFromSuperview();
    window.sendEvent(&escape_event(window, NSEventModifierFlags::empty()));
    assert_eq!(
        closed.load(Ordering::Relaxed),
        1,
        "Escape must close {}",
        window.title()
    );
    assert!(!window.isVisible());
    unsafe { center.removeObserver((*observer).as_ref()) };
}

pub fn verify_escape_autosave(settings: &SettingsWindow, saved: impl Fn() -> Config) {
    settings.select_tab(3);
    unsafe { settings.excluded.selectText(None) };
    let editor = settings
        .window
        .firstResponder()
        .unwrap()
        .downcast::<NSTextView>()
        .unwrap();
    editor.setString(ns_string!("Example Browser, Example Editor"));
    settings.window.sendEvent(&escape_event(
        &settings.window,
        NSEventModifierFlags::empty(),
    ));
    assert_eq!(saved().excluded_apps, ["Example Browser", "Example Editor"]);
}
