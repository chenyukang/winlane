use super::*;

pub(super) fn verify_quicklink_search(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.catalog_checked.set(Some(Instant::now()));
    state.mode.set(Some(PanelMode::Search));
    state.config.borrow_mut().quicklinks = vec![winlane::features::quicklinks::Quicklink {
        id: "rust-docs".into(),
        name: "Rust docs".into(),
        link: "https://example.com/{Query}".into(),
        open_with: String::new(),
    }];
    delegate.install_windows(vec![WindowInfo {
        id: 701,
        pid: -701,
        app: "Code".into(),
        title: "rust: main.rs".into(),
        minimized: false,
    }]);
    state.aliases.replace(Aliases::from_json(r#"{"apps":{"com.example.code":"r"},"windows":{"701":{"app":"com.example.code","alias":"r"}}}"#).unwrap());
    delegate.sync_displays();
    state.query.replace("r".into());
    delegate.filter();
    assert_eq!(state.quicklink_matches.borrow().len(), 1);
    assert_eq!(
        delegate.selected_window().unwrap().id,
        701,
        "window alias must stay first"
    );
    delegate.move_selection(1);
    assert_eq!(delegate.selected_quicklink().unwrap().id, "rust-docs");
    assert!(delegate.selected_window().is_none());
    assert!(delegate.selected_application().is_none());
    delegate.filter_preserving(delegate.selected_result());
    assert_eq!(delegate.selected_quicklink().unwrap().id, "rust-docs");
    for ui in delegate.panels() {
        assert!(matches!(
            ui.rows.borrow()[1].content,
            Some(RowContent::Quicklink(_))
        ));
    }
    state.query.replace("quicklink".into());
    delegate.filter();
    assert_eq!(delegate.selected_command(), Some(CommandId::Quicklinks));
    delegate.activate_selected();
    assert!(delegate.searching_quicklinks());
    assert_eq!(delegate.match_count(), 1);
    assert!(state.matches.borrow().is_empty());
    state.catalog_checked.set(None);
    delegate.ensure_app_catalog();
    assert!(state.catalog_receiver.borrow().is_none());
    state.query.replace("no matches".into());
    delegate.filter();
    delegate.activate_selected();
    assert!(delegate.searching_quicklinks());
    assert_eq!(delegate.match_count(), 0);
    state.catalog_checked.set(Some(Instant::now()));
    delegate.cancel_search();
    assert!(!delegate.scoped_search());
    assert_eq!(state.query.borrow().as_str(), "quicklink");
    delegate.enter_snippet_search();
    assert!(state.quicklink_matches.borrow().is_empty());
    delegate.enter_scoped_search(SearchScope::Clipboard);
    assert!(state.quicklink_matches.borrow().is_empty());
    state.search_scope.set(None);
    state.mode.set(Some(PanelMode::Switch));
    state.query.replace("r".into());
    delegate.filter();
    assert!(state.quicklink_matches.borrow().is_empty());
    state.mode.set(Some(PanelMode::Search));
    state.query.borrow_mut().clear();
    delegate.filter();
    assert!(state.quicklink_matches.borrow().is_empty());
    assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));
    let settings = delegate.ensure_settings_window();
    settings.select_tab(8);
    let editor = delegate.ensure_quicklink_editor();
    assert_eq!(editor.view().window().unwrap(), settings.window);
    for child in editor.view().subviews() {
        let frame = child.frame();
        assert!(frame.origin.x >= 0.0 && frame.origin.y >= 0.0);
        assert!(
            frame.origin.x + frame.size.width <= editor.view().bounds().size.width + 1.0,
            "quicklink control too wide: {frame:?}"
        );
        assert!(
            frame.origin.y + frame.size.height <= editor.view().bounds().size.height + 1.0,
            "quicklink control too tall: {frame:?}"
        );
    }
    assert!(!settings.window.isVisible());
    crate::macos::ui::quicklinks::tests::verify_editor(mtm);
    verify_inline_quicklink(mtm);
    println!(
        "Quicklink search: alias priority, row selection, scope, back, empty results, and switch-mode isolation."
    );
}

fn inline_control(ui: &PanelUi, index: usize) -> Retained<NSControl> {
    let bar = ui.quicklink_bar.borrow();
    bar.view
        .subviews()
        .iter()
        .filter_map(|view| view.downcast::<NSControl>().ok())
        .find(|control| bar.index(control) == Some(index))
        .unwrap()
}

fn verify_inline_quicklink(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.catalog_checked.set(Some(Instant::now()));
    state.mode.set(Some(PanelMode::Search));
    state.config.borrow_mut().show_usage_hints = false;
    state.config.borrow_mut().quicklinks = vec![winlane::features::quicklinks::Quicklink {
        id: "search-example".into(),
        name: "Search Example".into(),
        link: "https://example.com/?q={Query}&lang={argument name=\"Language\" default=\"en\"}"
            .into(),
        open_with: String::new(),
    }];
    state.previous_pid.set(-700);
    state.session.set(42);
    delegate.sync_displays();
    let ui = delegate.panels()[0].clone();
    let editor = NSTextView::initWithFrame(NSTextView::alloc(mtm), rect(0.0, 0.0, 100.0, 30.0));
    let command = |control: &NSControl, selector| {
        delegate
            .text_command(
                sel!(control:textView:doCommandBySelector:),
                control,
                &editor,
                selector,
            )
            .as_bool()
    };
    let windows_before = NSApplication::sharedApplication(mtm).windows().len();
    state.query.replace("Search Example".into());
    delegate.filter();
    assert!(
        command(&ui.input, sel!(insertTab:)),
        "Tab should enter inline parameters"
    );
    assert!(delegate.editing_quicklink());
    assert!(
        state.snippet_arguments.borrow().is_none(),
        "must not create a separate arguments window"
    );
    assert_eq!(
        NSApplication::sharedApplication(mtm).windows().len(),
        windows_before
    );
    assert_eq!(state.previous_pid.get(), -700);
    assert_eq!(state.session.get(), 42);
    assert_eq!(state.mode.get(), Some(PanelMode::Search));
    assert_eq!(delegate.match_count(), 1);
    assert!(ui.input.isHidden());
    assert!(!ui.quicklink_bar.borrow().view.isHidden());
    assert!(ui.scope_back.isHidden());
    let query = inline_control(&ui, 0);
    let language = inline_control(&ui, 1);
    assert_eq!(
        query
            .downcast_ref::<NSTextField>()
            .unwrap()
            .placeholderString()
            .unwrap()
            .to_string(),
        "Query"
    );
    assert_eq!(language.stringValue().to_string(), "en");
    delegate.activate_selected();
    assert!(
        delegate.editing_quicklink(),
        "missing parameters must keep the panel open"
    );
    assert!(
        state
            .quicklink_input
            .borrow()
            .as_ref()
            .unwrap()
            .error
            .is_some()
    );
    assert!(
        ui.rows.borrow()[0]
            .title
            .stringValue()
            .to_string()
            .contains("Query"),
        "validation must remain visible with hints off"
    );
    query.setStringValue(ns_string!("hello 世界 &"));
    let notification = unsafe {
        NSNotification::notificationWithName_object(
            ns_string!("NSControlTextDidChangeNotification"),
            Some(&query),
        )
    };
    delegate.text_changed(sel!(controlTextDidChange:), &notification);
    let destination = "https://example.com/?q=hello%20%E4%B8%96%E7%95%8C%20%26&lang=en";
    assert_eq!(
        state
            .quicklink_input
            .borrow()
            .as_ref()
            .unwrap()
            .destination()
            .unwrap(),
        destination
    );
    assert!(
        state
            .quicklink_input
            .borrow()
            .as_ref()
            .unwrap()
            .error
            .is_none()
    );
    assert_eq!(
        ui.rows.borrow()[0].title.stringValue().to_string(),
        destination
    );
    for other in delegate.panels() {
        assert_eq!(
            inline_control(&other, 0).stringValue().to_string(),
            "hello 世界 &"
        );
    }
    delegate.filter_preserving(None);
    assert_eq!(
        state
            .quicklink_input
            .borrow()
            .as_ref()
            .unwrap()
            .destination()
            .unwrap(),
        destination,
        "background updates must retain the draft"
    );
    state.catalog_checked.set(None);
    delegate.ensure_app_catalog();
    assert!(state.catalog_receiver.borrow().is_none());
    state.catalog_checked.set(Some(Instant::now()));
    assert_eq!(state.query.borrow().as_str(), "Search Example");
    delegate.toggle_mode(0);
    assert_eq!(
        state.mode.get(),
        Some(PanelMode::Search),
        "Space must remain text in parameters"
    );
    assert!(command(&query, sel!(insertTab:)));
    assert_eq!(state.quicklink_input.borrow().as_ref().unwrap().active, 1);
    assert!(command(&language, sel!(insertBacktab:)));
    assert_eq!(state.quicklink_input.borrow().as_ref().unwrap().active, 0);
    assert!(
        !command(&query, sel!(moveDown:)),
        "arrow keys belong to the native editor"
    );
    assert!(
        !command(&query, sel!(deleteBackward:)),
        "nonempty Backspace should delete text"
    );
    unsafe {
        NSTextInputClient::setMarkedText_selectedRange_replacementRange(
            &*editor,
            ns_string!("zhong"),
            objc2_foundation::NSRange::new(5, 0),
            objc2_foundation::NSRange::new(objc2_foundation::NSNotFound as usize, 0),
        );
    }
    assert!(
        !command(&query, sel!(insertNewline:)),
        "IME Enter must confirm composition, not open a link"
    );
    assert!(!command(&query, sel!(cancelOperation:)));
    assert!(delegate.editing_quicklink());
    NSTextInputClient::unmarkText(&*editor);
    assert!(command(&query, sel!(cancelOperation:)));
    assert!(!delegate.editing_quicklink());
    assert_eq!(state.query.borrow().as_str(), "Search Example");
    assert_eq!(delegate.selected_quicklink().unwrap().id, "search-example");
    assert!(!ui.input.isHidden());
    assert!(ui.quicklink_bar.borrow().view.isHidden());
    assert!(ui.quicklink_bar.borrow().control(0).is_none());
    assert_ne!(
        ui.rows.borrow()[0].title.stringValue().to_string(),
        destination,
        "returning must clear the inline preview"
    );

    delegate.enter_scoped_search(SearchScope::Quicklinks);
    state.query.replace("Example".into());
    delegate.filter();
    delegate.activate_selected();
    assert!(
        delegate.editing_quicklink(),
        "Enter also enters inline input"
    );
    let query = inline_control(&ui, 0);
    assert_eq!(
        query.stringValue().to_string(),
        "",
        "reopening must start a fresh draft"
    );
    assert!(command(&query, sel!(deleteBackward:)));
    assert!(!delegate.editing_quicklink());
    assert!(
        delegate.searching_quicklinks(),
        "back must preserve the Quicklinks scope"
    );
    assert_eq!(state.query.borrow().as_str(), "Example");

    for density in [DisplayDensity::Compact, DisplayDensity::Normal] {
        state.config.borrow_mut().display_density = density;
        let placeholders: Vec<_> = (0..8).map(|i| format!("{{Field {i}}}")).collect();
        let link = winlane::features::quicklinks::Quicklink {
            id: "many".into(),
            name: "Several parameters".into(),
            link: format!("https://example.com/{}", placeholders.join("/")),
            open_with: String::new(),
        };
        delegate.begin_quicklink_input(
            link.clone(),
            winlane::features::quicklinks::Template::parse(&link.link).unwrap(),
        );
        let bar = ui.quicklink_bar.borrow();
        for i in 0..8 {
            let frame = bar.control(i).unwrap().frame();
            assert!(frame.origin.x >= 0.0 && frame.origin.y >= 0.0);
            assert!(frame.origin.x + frame.size.width <= bar.view.bounds().size.width);
            assert!(frame.origin.y + frame.size.height <= bar.view.bounds().size.height);
        }
        assert!(
            ui.scroll.frame().origin.y + ui.scroll.frame().size.height <= bar.view.frame().origin.y
        );
        drop(bar);
        assert!(command(&inline_control(&ui, 0), sel!(insertBacktab:)));
        assert_eq!(state.quicklink_input.borrow().as_ref().unwrap().active, 7);
        assert!(command(&inline_control(&ui, 7), sel!(deleteBackward:)));
        assert_eq!(state.quicklink_input.borrow().as_ref().unwrap().active, 6);
        delegate.cancel_search();
    }
    let link = winlane::features::quicklinks::Quicklink { id: "choice".into(), name: "Choice".into(), link: r#"https://example.com/{argument name="Tone" type="choice" options="Friendly\nFormal" default="Formal"}"#.into(), open_with: String::new() };
    delegate.begin_quicklink_input(
        link.clone(),
        winlane::features::quicklinks::Template::parse(&link.link).unwrap(),
    );
    assert_eq!(
        state
            .quicklink_input
            .borrow()
            .as_ref()
            .unwrap()
            .destination()
            .unwrap(),
        "https://example.com/Formal"
    );
    let choice = inline_control(&ui, 0);
    choice
        .downcast_ref::<NSPopUpButton>()
        .unwrap()
        .selectItemAtIndex(1);
    assert!(delegate.update_quicklink_argument(&choice));
    assert_eq!(
        state
            .quicklink_input
            .borrow()
            .as_ref()
            .unwrap()
            .destination()
            .unwrap(),
        "https://example.com/Friendly"
    );
    delegate.display_search(state.session.get());
    assert!(!delegate.editing_quicklink());
    assert!(!delegate.scoped_search());
    state.query.replace("Search Example".into());
    delegate.filter();
    delegate.activate_selected();
    delegate.end_session();
    assert!(!delegate.editing_quicklink());
    assert!(ui.quicklink_bar.borrow().control(0).is_none());
    assert_eq!(
        NSApplication::sharedApplication(mtm).windows().len(),
        windows_before
    );
    assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));
    println!(
        "Inline Quicklinks: same window, parameters, encoding, IME, navigation, scope restoration, wrapping and lifecycle."
    );
}
