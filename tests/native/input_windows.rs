use super::*;
use crate::macos::ui::controls::{
    PreferencesWindow, input, label, preferences_window, rect, text_area,
};
use objc2::{DefinedClass, MainThreadOnly, msg_send};
use objc2_app_kit::NSView;
use objc2_foundation::{NSNotFound, NSRange, ns_string};
use winlane::core::input_method::InputMethod;

pub(crate) fn verify_shared_input(mtm: MainThreadMarker) {
    let before = Source::current(mtm).and_then(|source| source.id());
    let saved_policy = input_source::policy(mtm);
    let window = preferences_window(rect(0.0, 0.0, 400.0, 260.0), mtm);
    let root = NSView::initWithFrame(NSView::alloc(mtm), window.frame());
    window.setContentView(Some(&root));
    let field = input(rect(10.0, 210.0, 360.0, 28.0), "Name", mtm);
    let (scroll, multiline) = text_area(rect(10.0, 50.0, 360.0, 140.0), true, mtm);
    let caption = label("Name", 14.0, rect(10.0, 10.0, 100.0, 20.0), mtm);
    root.addSubview(&field);
    root.addSubview(&scroll);
    root.addSubview(&caption);
    assert!(editable(Some(&field)));
    assert!(editable(Some(&multiline)));
    assert!(!editable(Some(&caption)));
    let input = window.downcast_ref::<PreferencesWindow>().unwrap().ivars();
    for policy in [
        InputMethod::English,
        InputMethod::Chinese,
        InputMethod::Current,
        InputMethod::LastUsed,
    ] {
        input_source::set_policy(policy, mtm);
        let expected = input_source::preferred(policy, mtm).and_then(|source| source.id());
        for responder in [&*field as &NSResponder, &*multiline as &NSResponder] {
            assert!(window.makeFirstResponder(Some(responder)));
            let preference = input.preference.borrow();
            let preference = preference
                .as_ref()
                .expect("editable fields prepare the shared policy");
            assert_eq!(preference.target.as_ref().and_then(Source::id), expected);
            if matches!(policy, InputMethod::Current | InputMethod::Chinese) {
                assert!(preference.layout.is_none());
            }
            assert!(!input.session.borrow().focused);
            assert!(!window.isVisible());
        }
    }
    unsafe {
        NSTextInputClient::setMarkedText_selectedRange_replacementRange(
            &*multiline,
            ns_string!("ni"),
            NSRange::new(2, 0),
            NSRange::new(NSNotFound as usize, 0),
        );
    }
    assert!(composing(&window));
    input_source::align_editor(Source::for_language("en", mtm).as_ref(), &multiline);
    assert!(composing(&window));
    assert_eq!(multiline.string().to_string(), "ni");
    assert!(window.makeFirstResponder(Some(&*multiline)));
    assert!(
        composing(&window),
        "refocusing the same field must preserve composition"
    );
    // Refreshing a hidden window must not activate or remember an input source.
    unsafe {
        let _: () = msg_send![&*window, displayIfNeeded];
    }
    window.close();
    input_source::set_policy(saved_policy, mtm);
    assert_eq!(Source::current(mtm).and_then(|source| source.id()), before);
    println!(
        "Shared native input: single-line and multiline fields follow all four policies; hidden windows and composition leave the system input source unchanged."
    );
}
