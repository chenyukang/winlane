use super::*;

pub(super) fn verify_command_reapplies_input_policy(mtm: MainThreadMarker) {
    let before = Source::current(mtm).and_then(|source| source.id());
    let delegate = responsive_fixture(mtm);
    let state = delegate.ivars();
    delegate.prepare_panel(PanelMode::Search, 1, 0);
    let expected = input_source::preferred(InputMethod::English, mtm)
        .and_then(|source| source.id())
        .expect("English source is available for native checks");
    for scope in [
        SearchScope::Projects,
        SearchScope::OpenUrl,
        SearchScope::Quicklinks,
        SearchScope::Snippets,
        SearchScope::Clipboard,
    ] {
        delegate.cancel_input_start();
        state
            .input_session
            .borrow_mut()
            .prepare(Some("test.external-input".into()), InputMethod::English);
        state.input_session.borrow_mut().focused = true;
        state.input_session.borrow_mut().selected = Some("test.manually-selected-input".into());
        let (_projects_tx, projects_rx) = mpsc::channel();
        let (_history_tx, history_rx) = mpsc::channel();
        state.project_receiver.replace(Some(projects_rx));
        state.open_url_receiver.replace(Some(history_rx));
        state.changing_displays.set(true);
        delegate.enter_scoped_search(scope);
        state.changing_displays.set(false);
        assert_eq!(
            state.input_gate.borrow().target(),
            Some(expected.as_str()),
            "entering a command from an existing search must reapply the configured input source"
        );
        assert_eq!(
            state
                .input_session
                .borrow_mut()
                .finish(Some("test.manually-selected-input")),
            Some("test.external-input".into()),
            "a new command must preserve the original application's input source"
        );
    }
    delegate.end_session();
    assert_eq!(Source::current(mtm).and_then(|source| source.id()), before);
    for ui in delegate.panels() {
        ui.panel.close();
    }
}

pub(super) fn verify_input_language_is_prepared_before_focus(mtm: MainThreadMarker) {
    use winlane::core::input_method::InputMethod;

    let before = Source::current(mtm).and_then(|source| source.id());
    let delegate = responsive_fixture(mtm);
    for (policy, language) in [
        (InputMethod::English, Some("en")),
        (InputMethod::Chinese, Some("zh")),
        (InputMethod::Current, None),
        (InputMethod::LastUsed, None),
    ] {
        delegate.ivars().config.borrow_mut().input_method = policy;
        delegate.prepare_panel(PanelMode::Search, 1, 0);
        let layout_id = delegate
            .ivars()
            .input_target
            .borrow()
            .as_ref()
            .filter(|source| source.is_keyboard_layout())
            .and_then(Source::id);
        assert_eq!(*delegate.ivars().input_layout_source.borrow(), layout_id);
        if matches!(policy, InputMethod::Current | InputMethod::Chinese) {
            assert!(delegate.ivars().input_layout_source.borrow().is_none());
        }
        let expected = if policy == InputMethod::LastUsed {
            delegate
                .ivars()
                .input_target
                .borrow()
                .as_ref()
                .and_then(Source::primary_language)
        } else {
            language
                .filter(|_| delegate.ivars().input_target.borrow().is_some())
                .map(str::to_owned)
        };
        for ui in delegate.panels() {
            assert!(!ui.panel.isVisible());
            assert!(ui.input.currentEditor().is_none());
            let initial = ui.panel.initialFirstResponder().unwrap();
            assert!(
                initial.downcast_ref::<NSSearchField>().is_none(),
                "window activation must not focus a text editor before selecting the input source"
            );
            assert!(ui.panel.makeFirstResponder(Some(&initial)));
            assert!(initial.inputContext().is_none());
            assert!(ui.input.currentEditor().is_none());
            let cell = ui
                .input
                .cell()
                .unwrap()
                .downcast::<NSTextFieldCell>()
                .unwrap();
            let locales = cell.allowedInputSourceLocales();
            assert_eq!(
                locales.as_ref().map(|locales| locales
                    .iter()
                    .map(|locale| locale.to_string())
                    .collect::<Vec<_>>()),
                expected.as_ref().map(|locale| vec![locale.clone()]),
                "the first editor must have the requested language before AppKit focuses it"
            );
            let editor = NSTextView::initWithFrame(NSTextView::alloc(mtm), NSRect::ZERO);
            let editor = cell
                .setUpFieldEditorAttributes(&editor)
                .downcast::<NSTextView>()
                .unwrap();
            assert_eq!(editor.allowedInputSourceLocales(), locales);
        }
        let target = delegate
            .ivars()
            .input_gate
            .borrow()
            .target()
            .map(str::to_owned);
        delegate.ivars().changing_displays.set(true);
        delegate.complete_input_start();
        delegate.ivars().changing_displays.set(false);
        assert_eq!(
            delegate.ivars().input_gate.borrow().target(),
            target.as_deref(),
            "activation notifications must not remove the guard before the panel becomes key"
        );
        for scope in [
            SearchScope::OpenUrl,
            SearchScope::Projects,
            SearchScope::Quicklinks,
            SearchScope::Snippets,
            SearchScope::Clipboard,
        ] {
            let (_history_tx, history_rx) = mpsc::channel();
            let (_projects_tx, projects_rx) = mpsc::channel();
            delegate.ivars().open_url_receiver.replace(Some(history_rx));
            delegate.ivars().project_receiver.replace(Some(projects_rx));
            delegate.enter_scoped_search(scope);
            assert_eq!(
                delegate.ivars().input_gate.borrow().target(),
                target.as_deref(),
                "opening a command search before presentation must preserve the configured input source: {policy:?}"
            );
            assert_eq!(
                delegate
                    .ivars()
                    .input_target
                    .borrow()
                    .as_ref()
                    .and_then(Source::id),
                target,
            );
            assert!(!delegate.ivars().input_session.borrow().focused);
            assert!(delegate.ivars().input_start_timer.borrow().is_none());
            for ui in delegate.panels() {
                let cell = ui
                    .input
                    .cell()
                    .unwrap()
                    .downcast::<NSTextFieldCell>()
                    .unwrap();
                assert_eq!(
                    cell.allowedInputSourceLocales()
                        .as_ref()
                        .map(|locales| locales
                            .iter()
                            .map(|locale| locale.to_string())
                            .collect::<Vec<_>>()),
                    expected.as_ref().map(|locale| vec![locale.clone()]),
                    "command searches must retain the same startup language constraint as search"
                );
            }
        }
        delegate.clear_input_start_locales();
        assert_eq!(
            delegate.ivars().input_gate.borrow().target(),
            target.as_deref(),
            "removing the language constraint must keep keys gated until the unrestricted context is ready"
        );
        delegate.cancel_input_start();
        for ui in delegate.panels() {
            let cell = ui
                .input
                .cell()
                .unwrap()
                .downcast::<NSTextFieldCell>()
                .unwrap();
            assert!(
                cell.allowedInputSourceLocales().is_none(),
                "manual switching must be allowed after startup"
            );
        }
    }
    assert_eq!(
        Source::current(mtm).and_then(|source| source.id()),
        before,
        "preparing hidden panels must not switch the system input source"
    );
    for ui in delegate.panels() {
        ui.panel.close();
    }
    println!(
        "Input startup: language constraints are installed before focus, survive activation notifications, and are removed after startup; system input source unchanged."
    );
}

pub(super) fn verify_search_composition_survives_refresh(mtm: MainThreadMarker) {
    use objc2_foundation::{NSNotFound, NSRange};

    let delegate = responsive_fixture(mtm);
    delegate.ivars().config.borrow_mut().input_method =
        winlane::core::input_method::InputMethod::Chinese;
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
        assert_eq!(
            editor.string().to_string(),
            "ni",
            "refresh lost early pinyin"
        );
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
    assert!(
        ui.input.stringValue().is_empty(),
        "old composition leaked into a new search"
    );
    assert!(ui.input.currentEditor().is_none());
    assert!(
        ui.panel
            .firstResponder()
            .unwrap()
            .downcast_ref::<PanelContentView>()
            .is_some(),
        "reopening must reset the previous editor to a non-text responder"
    );
    for ui in delegate.panels() {
        ui.panel.makeFirstResponder(None);
        ui.panel.close();
    }
    println!(
        "Search composition: early pinyin survives refresh, committed text synchronizes, new searches clear old composition."
    );
}

pub(super) fn verify_direct_layout_input(mtm: MainThreadMarker) {
    use objc2_foundation::{NSNotFound, NSRange};
    let before = Source::current(mtm).and_then(|source| source.id());
    let delegate = responsive_fixture(mtm);
    delegate.ivars().config.borrow_mut().input_method = InputMethod::Current;
    delegate.prepare_panel(PanelMode::Search, 1, 0);
    let ui = delegate.panels().remove(0);
    assert!(ui.panel.makeFirstResponder(Some(&ui.input)));
    let editor = ui
        .input
        .currentEditor()
        .unwrap()
        .downcast::<NSTextView>()
        .unwrap();
    let key = |code, flags, chars: &str| {
        let chars = NSString::from_str(chars);
        NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::KeyDown, NSPoint::ZERO, flags, 0.0, ui.panel.windowNumber(),
            None, &chars, &chars, false, code,
        ).unwrap()
    };
    let plain = NSEventModifierFlags::empty();
    // Deliberately stale event text: translation must use the selected layout.
    let a = key(0, plain, "z");
    assert!(
        !delegate.insert_layout_key(&ui.panel, &a),
        "Current must keep native input"
    );
    delegate
        .ivars()
        .input_layout_source
        .replace(Some("test.unavailable.layout".into()));
    assert!(
        !delegate.insert_layout_key(&ui.panel, &a),
        "a different current source must keep native input"
    );
    delegate.ivars().input_layout_source.take();
    editor.setEditable(false);
    assert!(!insert_keyboard_layout_text(&editor, &a));
    editor.setEditable(true);
    assert!(insert_keyboard_layout_text(&editor, &a));
    assert_eq!(editor.string().to_string(), "a");
    assert!(!NSTextInputClient::hasMarkedText(&*editor));
    assert!(insert_keyboard_layout_text(
        &editor,
        &key(0, NSEventModifierFlags::Shift, "a")
    ));
    assert!(insert_keyboard_layout_text(
        &editor,
        &key(18, NSEventModifierFlags::Shift, "1")
    ));
    assert_eq!(editor.string().to_string(), "aA!");
    assert_eq!(delegate.ivars().query.borrow().as_str(), "aA!");
    for panel in delegate.panels() {
        assert_eq!(panel.input.stringValue().to_string(), "aA!");
    }
    editor.setSelectedRange(NSRange::new(1, 1));
    assert!(insert_keyboard_layout_text(&editor, &a));
    assert_eq!(editor.string().to_string(), "aa!");
    editor.setSelectedRange(NSRange::new(0, 1));
    assert!(insert_keyboard_layout_text(
        &editor,
        &key(0, NSEventModifierFlags::CapsLock, "a")
    ));
    assert_eq!(editor.string().to_string(), "Aa!");
    for (code, flags, text) in [
        (0, NSEventModifierFlags::Command, "a"),
        (0, NSEventModifierFlags::Control, "a"),
        (14, NSEventModifierFlags::Option, ""),
        (36, plain, "\r"),
        (51, plain, "\u{7f}"),
        (123, plain, "\u{f702}"),
    ] {
        assert!(!insert_keyboard_layout_text(
            &editor,
            &key(code, flags, text)
        ));
        assert_eq!(editor.string().to_string(), "Aa!");
    }
    // Actual IME composition, including Option dead keys, stays native.
    unsafe {
        NSTextInputClient::setMarkedText_selectedRange_replacementRange(
            &*editor,
            ns_string!("ni"),
            NSRange::new(2, 0),
            NSRange::new(NSNotFound as usize, 0),
        );
    }
    let composition = editor.string();
    assert!(!insert_keyboard_layout_text(&editor, &a));
    assert_eq!(editor.string(), composition);
    assert!(NSTextInputClient::hasMarkedText(&*editor));
    delegate.ivars().input_layout_source.replace(before.clone());
    delegate.finish_search_input();
    assert!(delegate.ivars().input_layout_source.borrow().is_none());
    for panel in delegate.panels() {
        panel.panel.close();
    }
    assert_eq!(Source::current(mtm).and_then(|source| source.id()), before);
    println!(
        "Direct layout input: immediate committed text, layout translation, Shift, selection, query sync, native shortcuts and IME composition; system input source unchanged."
    );
}
