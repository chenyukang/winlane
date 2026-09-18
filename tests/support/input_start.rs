fn verify_search_composition_survives_refresh(mtm: MainThreadMarker) {
    use objc2_foundation::{NSNotFound, NSRange};

    let delegate = responsive_fixture(mtm);
    delegate.ivars().config.borrow_mut().input_method =
        winlane::input_method::InputMethod::Chinese;
    delegate.ivars().mode.set(Some(PanelMode::Search));
    delegate.sync_displays();
    delegate.filter();
    let ui = delegate.panels().remove(0);
    assert!(ui.panel.makeFirstResponder(Some(&ui.input)));
    let editor = ui
        .input
        .currentEditor()
        .unwrap()
        .downcast::<NSTextView>()
        .unwrap();
    // Exercise the real field editor without switching the system input source.
    unsafe {
        NSTextInputClient::setMarkedText_selectedRange_replacementRange(
            &*editor,
            ns_string!("ni"),
            NSRange::new(2, 0),
            NSRange::new(NSNotFound as usize, 0),
        );
    }
    assert!(NSTextInputClient::hasMarkedText(&*editor));
    assert_eq!(editor.string().to_string(), "ni");
    for _ in 0..3 {
        delegate.filter_preserving(delegate.selected_result());
        assert_eq!(editor.string().to_string(), "ni", "refresh lost early pinyin");
        assert!(NSTextInputClient::hasMarkedText(&*editor));
    }
    unsafe {
        NSTextInputClient::insertText_replacementRange(
            &*editor,
            ns_string!("你"),
            NSRange::new(NSNotFound as usize, 0),
        );
    }
    assert!(!NSTextInputClient::hasMarkedText(&*editor));
    assert_eq!(editor.string().to_string(), "你");
    assert_eq!(delegate.ivars().query.borrow().as_str(), "你");
    for ui in delegate.panels() {
        assert_eq!(ui.input.stringValue().to_string(), "你");
    }
    unsafe {
        NSTextInputClient::setMarkedText_selectedRange_replacementRange(
            &*editor,
            ns_string!("hao"),
            NSRange::new(3, 0),
            NSRange::new(NSNotFound as usize, 0),
        );
    }
    assert!(NSTextInputClient::hasMarkedText(&*editor));
    delegate.end_session();
    delegate.prepare_panel(PanelMode::Search, 2, 0);
    assert!(ui.input.stringValue().is_empty(), "old composition leaked into a new search");
    for ui in delegate.panels() {
        ui.panel.makeFirstResponder(None);
        ui.panel.close();
    }
    println!("Search composition: early pinyin survives refresh, committed text synchronizes, new searches clear old composition.");
}
