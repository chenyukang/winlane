use super::*;
use winlane::features::open_url::{History, InputTarget, Page};

pub(super) fn verify_open_url(mtm: MainThreadMarker) {
    verify_context_menu_key(mtm);
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.catalog_checked.set(Some(Instant::now()));
    state.mode.set(Some(PanelMode::Search));
    delegate.sync_displays();
    let windows_before = NSApplication::sharedApplication(mtm).windows().len();
    let pages = vec![
        Page {
            url: "https://example.com/rust?a=1&b=2#part".into(),
            title: "Rust 文档".into(),
            last_visit_time: 20,
        },
        Page {
            url: "https://example.test/guide".into(),
            title: "Guide".into(),
            last_visit_time: 10,
        },
    ];
    state.query.replace("rust".into());
    delegate.filter();
    assert!(state.open_url_receiver.borrow().is_none());
    assert!(state.open_url_matches.borrow().is_empty());
    state.query.replace("open-url".into());
    delegate.filter();
    assert_eq!(delegate.selected_command(), Some(CommandId::OpenUrl));
    assert!(
        state.open_url_receiver.borrow().is_none(),
        "matching the command must not read history"
    );
    let (tx, rx) = mpsc::channel();
    state.open_url_receiver.replace(Some(rx));
    delegate.activate_selected();
    assert!(delegate.searching_open_url());
    assert_eq!(delegate.match_count(), 0);
    assert!(delegate.panels().iter().all(|ui| !ui.scope_back.isHidden()));
    state.query.replace("文档".into());
    delegate.filter();
    assert!(
        matches!(
            delegate.open_url_target(false),
            Some(InputTarget::Search(_))
        ),
        "input can search even while history loads"
    );
    tx.send(History {
        pages: pages.clone(),
        error: None,
        access_denied: false,
    })
    .unwrap();
    delegate.poll_open_url();
    assert_eq!(
        delegate.match_count(),
        1,
        "typing during loading must filter the arriving result"
    );
    assert_eq!(
        delegate.selected_url(),
        Some(pages[0].clone()),
        "matching results should select the first URL even when they arrive after typing"
    );
    assert!(
        delegate
            .panels()
            .iter()
            .all(|ui| ui.rows.borrow()[0].selected == Some(true))
    );
    delegate.move_selection(1);
    assert_eq!(delegate.selected_url(), Some(pages[0].clone()));
    assert_eq!(
        delegate.open_url_target(false),
        Some(InputTarget::Url(pages[0].url.clone()))
    );
    delegate.move_selection(-1);
    assert_eq!(delegate.selected_url(), Some(pages[0].clone()));
    let ui = delegate.panels()[0].clone();
    assert!(matches!(
        delegate.open_url_target(true),
        Some(InputTarget::Search(_))
    ));
    assert_eq!(
        delegate.selected_url(),
        Some(pages[0].clone()),
        "using input does not change selection"
    );
    verify_input_keys(&delegate, &ui);

    state.query.replace("example.com".into());
    delegate.filter();
    assert_eq!(
        delegate.open_url_target(false),
        Some(InputTarget::Url(pages[0].url.clone()))
    );
    assert_eq!(
        delegate.open_url_target(true),
        Some(InputTarget::Url("https://example.com/".into()))
    );
    state.query.replace("example".into());
    delegate.filter();
    assert_eq!(delegate.selected_url(), Some(pages[0].clone()));
    delegate.move_selection(1);
    assert_eq!(delegate.selected_url(), Some(pages[1].clone()));
    assert_eq!(
        delegate.open_url_target(false),
        Some(InputTarget::Url(pages[1].url.clone()))
    );
    assert!(matches!(
        delegate.open_url_target(true),
        Some(InputTarget::Search(_))
    ));
    delegate.move_selection(1);
    assert_eq!(
        delegate.selected_url(),
        Some(pages[0].clone()),
        "arrows wrap between rows without an input-only slot"
    );
    state.query.replace("文档".into());
    delegate.filter();
    assert_eq!(
        delegate.selected_url(),
        Some(pages[0].clone()),
        "editing selects the first matching URL"
    );
    assert!(
        ui.panel.makeFirstResponder(Some(&ui.input)),
        "focusing the input must keep the matched URL selected"
    );
    assert_eq!(delegate.selected_url(), Some(pages[0].clone()));
    assert!(state.matches.borrow().is_empty());
    assert!(state.command_matches.borrow().is_empty());
    assert!(state.project_matches.borrow().is_empty());
    assert!(state.quicklink_matches.borrow().is_empty());
    assert!(delegate.selected_window().is_none());
    assert!(delegate.selected_project().is_none());
    for ui in delegate.panels() {
        assert_eq!(
            ui.rows.borrow()[0].app.stringValue().to_string(),
            "example.com"
        );
        assert!(
            ui.rows.borrow()[0]
                .title
                .stringValue()
                .to_string()
                .contains("Rust 文档")
        );
        assert!(matches!(
            ui.rows.borrow()[0].content,
            Some(RowContent::OpenUrl(_))
        ));
    }
    state.query.borrow_mut().clear();
    delegate.filter();
    assert!(
        delegate.open_url_target(true).is_none(),
        "Ctrl+Enter with empty input does nothing"
    );
    delegate.submit_open_url(true);
    assert!(delegate.searching_open_url());
    delegate.move_selection(1);
    let (tx, rx) = mpsc::channel();
    state.open_url_receiver.replace(Some(rx));
    tx.send(History {
        pages: pages.iter().rev().cloned().collect(),
        error: Some("Partial history error".into()),
        access_denied: false,
    })
    .unwrap();
    state.config.borrow_mut().show_usage_hints = false;
    delegate.poll_open_url();
    assert_eq!(
        delegate.selected_url(),
        Some(pages[1].clone()),
        "refresh must preserve URL identity"
    );
    assert!(delegate.panels().iter().all(|ui| !ui.footer.isHidden()));
    assert!(
        delegate
            .panels()
            .iter()
            .all(|ui| ui.history_permissions_button.isHidden())
    );
    state.open_url_history.borrow_mut().access_denied = true;
    delegate.render();
    assert!(
        delegate.panels().iter().all(|ui| {
            !ui.history_permissions_button.isHidden()
                && ui.settings_button.isHidden()
                && ui.footer.frame().origin.x + ui.footer.frame().size.width
                    < ui.history_permissions_button.frame().origin.x
        }),
        "access repair must be visible even with partial results and usage hints hidden"
    );
    state.query.borrow_mut().clear();
    state.open_url_history.borrow_mut().pages.clear();
    delegate.filter();
    assert!(delegate.panels().iter().all(|ui| {
        ui.empty_labels
            .borrow()
            .iter()
            .any(|label| label.stringValue().to_string().contains("完全磁盘访问权限"))
    }));
    let (tx, rx) = mpsc::channel();
    state.open_url_receiver.replace(Some(rx));
    tx.send(History {
        pages: pages.clone(),
        ..History::default()
    })
    .unwrap();
    delegate.poll_open_url();
    assert!(
        delegate
            .panels()
            .iter()
            .all(|ui| ui.history_permissions_button.isHidden()),
        "a successful retry must clear permission guidance"
    );
    state.catalog_checked.set(None);
    delegate.ensure_app_catalog();
    assert!(state.catalog_receiver.borrow().is_none());
    state.catalog_checked.set(Some(Instant::now()));
    delegate.toggle_mode(0);
    assert!(delegate.searching_open_url());
    state.query.replace("absent".into());
    delegate.filter();
    assert!(matches!(
        delegate.open_url_target(false),
        Some(InputTarget::Search(_))
    ));
    assert!(delegate.panels().iter().all(|ui| {
        ui.empty_labels
            .borrow()
            .iter()
            .any(|label| label.stringValue().to_string().contains("Google"))
    }));
    state.query.replace("example.com/new?q=中文&x=1".into());
    delegate.filter();
    assert_eq!(
        delegate.open_url_target(false),
        Some(InputTarget::Url(
            "https://example.com/new?q=%E4%B8%AD%E6%96%87&x=1".into()
        ))
    );
    assert!(delegate.searching_open_url());
    assert_eq!(delegate.match_count(), 0);
    assert_eq!(
        delegate.open_url_target(false),
        delegate.open_url_target(true),
        "both actions use input when nothing matches"
    );
    delegate.leave_scoped_search();
    assert_eq!(state.query.borrow().as_str(), "open-url");
    assert_eq!(delegate.selected_command(), Some(CommandId::OpenUrl));
    assert_eq!(state.open_url_history.borrow().pages.len(), 2);
    assert!(
        delegate
            .panels()
            .iter()
            .all(|ui| ui.history_permissions_button.isHidden())
    );
    assert!(state.open_url_matches.borrow().is_empty());
    assert!(
        delegate
            .panels()
            .iter()
            .all(|ui| ui.rows.borrow().iter().all(|row| {
                let button: &NSButton = &row.button;
                NSAccessibility::accessibilityLabel(button)
                    .is_none_or(|label| !label.to_string().contains("example."))
            })),
        "leaving must clear retained accessibility descriptions too"
    );
    let (tx, rx) = mpsc::channel();
    state.open_url_receiver.replace(Some(rx));
    delegate.activate_selected();
    let editor = NSTextView::initWithFrame(NSTextView::alloc(mtm), rect(0.0, 0.0, 100.0, 30.0));
    let ui = delegate.panels()[0].clone();
    assert!(
        !delegate
            .text_command(
                sel!(control:textView:doCommandBySelector:),
                &ui.input,
                &editor,
                sel!(deleteBackward:)
            )
            .as_bool()
    );
    assert!(delegate.searching_open_url());
    delegate.leave_scoped_search();
    tx.send(History {
        pages: pages.clone(),
        ..History::default()
    })
    .unwrap();
    let before = state.render_passes.get();
    delegate.poll_open_url();
    assert_eq!(delegate.selected_command(), Some(CommandId::OpenUrl));
    assert_eq!(
        state.render_passes.get(),
        before,
        "late completion only updates the cache"
    );
    assert_eq!(state.open_url_history.borrow().pages, pages);
    let (tx, rx) = mpsc::channel();
    state.open_url_receiver.replace(Some(rx));
    delegate.activate_selected();
    assert_eq!(
        delegate.match_count(),
        2,
        "show cached rows while refreshing"
    );
    assert_eq!(delegate.selected_url(), Some(pages[0].clone()));
    tx.send(History {
        error: Some("History temporarily unavailable".into()),
        access_denied: true,
        ..History::default()
    })
    .unwrap();
    delegate.poll_open_url();
    assert_eq!(state.open_url_history.borrow().pages, pages);
    assert_eq!(delegate.selected_url(), Some(pages[0].clone()));
    assert!(
        delegate
            .panels()
            .iter()
            .all(|ui| !ui.history_permissions_button.isHidden())
    );
    let (tx, rx) = mpsc::channel();
    state.open_url_receiver.replace(Some(rx));
    delegate.end_session();
    assert_eq!(state.open_url_history.borrow().pages, pages);
    assert!(state.open_url_matches.borrow().is_empty());
    assert!(state.open_url_receiver.borrow().is_some());
    assert!(
        delegate
            .panels()
            .iter()
            .all(|ui| ui.rows.borrow().is_empty())
    );
    tx.send(History::default()).unwrap();
    delegate.poll_open_url();
    assert!(
        state.open_url_history.borrow().pages.is_empty(),
        "a successful empty refresh clears old history"
    );
    assert!(state.open_url_history.borrow().error.is_none());
    assert!(!state.open_url_history.borrow().access_denied);
    assert!(state.open_url_receiver.borrow().is_none());
    state.mode.set(Some(PanelMode::Switch));
    delegate.filter();
    assert!(state.open_url_matches.borrow().is_empty());
    let prepared = crate::macos::platform::open_url::PreparedPage::new(&pages[0].url);
    if NSWorkspace::sharedWorkspace()
        .URLForApplicationWithBundleIdentifier(&NSString::from_str("com.google.Chrome"))
        .is_some()
    {
        assert_eq!(
            prepared.unwrap().url.absoluteString().unwrap().to_string(),
            pages[0].url
        );
    } else {
        assert!(prepared.is_err());
    }
    let invalid = Page {
        url: "javascript:alert(1)".into(),
        ..pages[0].clone()
    };
    assert!(crate::macos::platform::open_url::PreparedPage::new(&invalid.url).is_err());
    assert_eq!(
        NSApplication::sharedApplication(mtm).windows().len(),
        windows_before
    );
    assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));
    println!(
        "Open URL: cached entry, asynchronous refresh, MRU search, selection retention, error display, late completion and row cleanup; no browser opened."
    );
}

fn verify_input_keys(delegate: &Delegate, ui: &PanelUi) {
    let key = |code, flags| {
        NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::KeyDown, NSPoint::ZERO, flags, 0.0, ui.panel.windowNumber(),
            None, ns_string!("\r"), ns_string!("\r"), false, code,
        ).unwrap()
    };
    let control = NSEventModifierFlags::Control;
    for code in [36, 76] {
        let event = key(code, control);
        assert!(delegate.is_open_url_input_key(&event, false));
        assert!(
            !delegate.is_open_url_input_key(&event, true),
            "IME composition owns Return"
        );
        assert!(matches!(
            delegate.open_url_target(delegate.is_open_url_input_key(&event, false)),
            Some(InputTarget::Search(_))
        ));
        for flags in [
            NSEventModifierFlags::empty(),
            NSEventModifierFlags::Command,
            NSEventModifierFlags::Option,
            control | NSEventModifierFlags::Command,
            control | NSEventModifierFlags::Shift,
            control | NSEventModifierFlags::Option,
        ] {
            assert!(!delegate.is_open_url_input_key(&key(code, flags), false));
        }
        assert!(
            delegate
                .is_open_url_input_key(&key(code, control | NSEventModifierFlags::CapsLock), false)
        );
    }
    assert!(!delegate.is_open_url_input_key(&key(48, control), false));
    let state = delegate.ivars();
    for scope in [
        None,
        Some(SearchScope::Projects),
        Some(SearchScope::Quicklinks),
        Some(SearchScope::Snippets),
        Some(SearchScope::Clipboard),
    ] {
        state.search_scope.set(scope);
        assert!(
            !delegate.is_open_url_input_key(&key(36, control), false),
            "Ctrl+Enter is scoped to open-url"
        );
        assert!(delegate.open_url_target(true).is_none());
    }
    state.search_scope.set(Some(SearchScope::OpenUrl));
}

define_class!(
    // SAFETY: This test-only editor prevents native menu tracking in hidden checks.
    #[unsafe(super = NSTextView)]
    #[thread_kind = MainThreadOnly]
    struct ContextMenuTestEditor;
    unsafe impl NSObjectProtocol for ContextMenuTestEditor {}
    impl ContextMenuTestEditor {
        #[unsafe(method(showContextMenuForSelection:))]
        fn show_context_menu(&self, _: Option<&AnyObject>) {}
        #[unsafe(method(menuForEvent:))]
        fn menu(&self, _: &NSEvent) -> *mut NSMenu { std::ptr::null_mut() }
    }
);

fn verify_context_menu_key(mtm: MainThreadMarker) {
    use objc2_foundation::{NSNotFound, NSRange};

    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.mode.set(Some(PanelMode::Search));
    state.search_scope.set(Some(SearchScope::OpenUrl));
    delegate.sync_displays();
    let ui = delegate.panels()[0].clone();
    let test_editor: Retained<ContextMenuTestEditor> = unsafe {
        msg_send![super(ContextMenuTestEditor::alloc(mtm).set_ivars(())), initWithFrame: NSRect::ZERO]
    };
    let editor: &NSTextView = &test_editor;
    ui.panel.contentView().unwrap().addSubview(editor);
    assert!(ui.panel.makeFirstResponder(Some(editor)));
    // SAFETY: The panel outlives this editor; use the normal editor-to-window chain.
    unsafe {
        editor.setNextResponder(Some(&ui.panel));
    }
    if !editor.respondsToSelector(sel!(contextMenuKeyDown:)) {
        return;
    }
    let key = |code, flags, text: &NSString| {
        NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::KeyDown, NSPoint::ZERO, flags, 0.0, ui.panel.windowNumber(),
            None, text, text, false, code,
        ).unwrap()
    };
    let control = NSEventModifierFlags::Control;
    for scope in [SearchScope::OpenUrl, SearchScope::Files] {
        state.search_scope.set(Some(scope));
        for code in [36, 76] {
            let event = key(code, control, ns_string!("\r"));
            editor.contextMenuKeyDown(&event);
            assert!(
                state.launch_receiver.borrow().is_none(),
                "empty input must not launch anything"
            );

            // Input still starting: submit must stay behind the letters already queued.
            state.changing_displays.set(true);
            state
                .input_gate
                .borrow_mut()
                .begin(Some("test.pending-input".into()));
            ui.panel
                .sendEvent(&key(0, NSEventModifierFlags::empty(), ns_string!("a")));
            editor.contextMenuKeyDown(&event);
            let queued = state.input_gate.borrow_mut().finish(None, true).unwrap();
            assert_eq!(
                queued
                    .iter()
                    .map(|event| event.keyCode())
                    .collect::<Vec<_>>(),
                [0, code]
            );
            delegate.cancel_input_start();
            state.changing_displays.set(false);
        }
    }

    state.changing_displays.set(true);
    for scope in [
        None,
        Some(SearchScope::Projects),
        Some(SearchScope::Quicklinks),
        Some(SearchScope::Snippets),
        Some(SearchScope::Clipboard),
    ] {
        state.search_scope.set(scope);
        state
            .input_gate
            .borrow_mut()
            .begin(Some("test.pending-input".into()));
        editor.contextMenuKeyDown(&key(36, control, ns_string!("\r")));
        assert!(
            state
                .input_gate
                .borrow_mut()
                .finish(None, true)
                .unwrap()
                .is_empty(),
            "other scopes must not queue a URL submission"
        );
    }
    state.search_scope.set(Some(SearchScope::OpenUrl));
    state
        .input_gate
        .borrow_mut()
        .begin(Some("test.pending-input".into()));
    editor.contextMenuKeyDown(&key(36, NSEventModifierFlags::Option, ns_string!("\r")));
    assert!(
        state
            .input_gate
            .borrow_mut()
            .finish(None, true)
            .unwrap()
            .is_empty(),
        "a customized system menu shortcut must not queue a URL submission"
    );
    unsafe {
        NSTextInputClient::setMarkedText_selectedRange_replacementRange(
            editor,
            ns_string!("ni"),
            NSRange::new(2, 0),
            NSRange::new(NSNotFound as usize, 0),
        );
    }
    assert!(NSTextInputClient::hasMarkedText(editor));
    assert!(ui.panel.makeFirstResponder(Some(editor)));
    state
        .input_gate
        .borrow_mut()
        .begin(Some("test.pending-input".into()));
    editor.contextMenuKeyDown(&key(36, control, ns_string!("\r")));
    assert!(
        state
            .input_gate
            .borrow_mut()
            .finish(None, true)
            .unwrap()
            .is_empty(),
        "composition must not queue a URL submission"
    );
    assert!(NSTextInputClient::hasMarkedText(editor));
    assert!(state.launch_receiver.borrow().is_none());
    delegate.cancel_input_start();
    state.changing_displays.set(false);
    ui.panel.makeFirstResponder(None);
    unsafe {
        editor.setNextResponder(None);
    }
    editor.removeFromSuperview();
    assert!(!ui.panel.isVisible());
    ui.panel.close();
}
