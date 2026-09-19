use super::*;
use winlane::features::open_url::{History, InputTarget, Page};

pub(super) fn verify_open_url(mtm: MainThreadMarker) {
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
        matches!(delegate.open_url_target(), Some(InputTarget::Search(_))),
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
    assert!(
        delegate.selected_url().is_none(),
        "loading results must not select a URL over typed input"
    );
    assert!(
        delegate
            .panels()
            .iter()
            .all(|ui| ui.rows.borrow()[0].selected == Some(false))
    );
    delegate.move_selection(1);
    assert_eq!(delegate.selected_url(), Some(pages[0].clone()));
    assert_eq!(
        delegate.open_url_target(),
        Some(InputTarget::Url(pages[0].url.clone()))
    );
    delegate.move_selection(-1);
    assert!(matches!(
        delegate.open_url_target(),
        Some(InputTarget::Search(_))
    ));
    delegate.move_selection(-1);
    assert_eq!(delegate.selected_url(), Some(pages[0].clone()));
    let ui = delegate.panels()[0].clone();
    let input_point = ui.input.convertPoint_toView(NSPoint::new(20.0, 10.0), None);
    delegate.focus_open_url_input_at(&ui.panel, input_point);
    assert!(
        delegate.selected_url().is_none(),
        "clicking the input clears the history selection"
    );
    delegate.move_selection(1);
    delegate.filter();
    assert!(
        delegate.selected_url().is_none(),
        "editing returns Enter to the input"
    );
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
        delegate.open_url_target(),
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
        delegate.open_url_target(),
        Some(InputTarget::Url(
            "https://example.com/new?q=%E4%B8%AD%E6%96%87&x=1".into()
        ))
    );
    assert!(delegate.searching_open_url());
    assert_eq!(delegate.match_count(), 0);
    delegate.cancel_search();
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
        delegate
            .text_command(
                sel!(control:textView:doCommandBySelector:),
                &ui.input,
                &editor,
                sel!(deleteBackward:)
            )
            .as_bool()
    );
    assert!(!delegate.scoped_search());
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
