use super::*;

pub(super) fn verify_emoji(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.catalog_checked.set(Some(Instant::now()));
    state.mode.set(Some(PanelMode::Search));
    state.previous_pid.set(-12345);
    delegate.sync_displays();
    state.query.replace("emoji".into());
    delegate.filter();
    assert_eq!(delegate.selected_command(), Some(CommandId::Emoji));
    assert!(state.emoji_matches.borrow().is_empty());
    delegate.activate_selected();
    assert!(delegate.searching_emoji());
    assert_eq!(
        state.previous_pid.get(),
        -12345,
        "scope entry retains the paste target"
    );
    assert_eq!(
        delegate.match_count(),
        winlane::features::emoji::MAX_RESULTS
    );
    assert!(state.command_matches.borrow().is_empty());
    assert!(delegate.selected_window().is_none());
    assert!(delegate.selected_application().is_none());
    let first = delegate.selected_emoji().unwrap();
    for ui in delegate.panels() {
        let rows = ui.rows.borrow();
        assert_eq!(rows[0].app.stringValue().to_string(), first.text);
        assert_eq!(rows[0].title.stringValue().to_string(), first.name());
        assert!(rows[0].button.isAccessibilitySelected());
        assert!(rows[0].app.frame().origin.x < rows[0].title.frame().origin.x);
        assert!(rows[0].alias.isHidden() && rows[0].icon.isHidden());
        assert!(!ui.panel.isVisible());
    }
    delegate.move_selection(1);
    let selected = delegate.selected_emoji().unwrap();
    delegate.filter_preserving(delegate.selected_result());
    assert_eq!(delegate.selected_emoji(), Some(selected));

    for (query, expected) in [("火箭", "🚀"), ("rocket", "🚀"), ("👩🏽‍💻", "👩🏽‍💻")]
    {
        let ui = delegate.panels()[0].clone();
        ui.input.setStringValue(&NSString::from_str(query));
        let notification = unsafe {
            NSNotification::notificationWithName_object(
                ns_string!("NSControlTextDidChangeNotification"),
                Some(&ui.input),
            )
        };
        unsafe {
            let _: () = msg_send![&*delegate, controlTextDidChange: &*notification];
        }
        assert_eq!(delegate.selected_emoji().unwrap().text, expected);
        assert_eq!(*state.query.borrow(), query);
        assert_eq!(state.previous_pid.get(), -12345);
    }
    let before = NSPasteboard::generalPasteboard().changeCount();
    delegate.activate_selected();
    assert!(
        delegate.searching_emoji(),
        "a missing target keeps the picker open"
    );
    assert!(state.snippet_paste_timer.borrow().is_none());
    assert_eq!(NSPasteboard::generalPasteboard().changeCount(), before);

    state.query.replace("no-such-emoji-xyz".into());
    delegate.filter();
    assert_eq!(delegate.match_count(), 0);
    delegate.activate_selected();
    assert!(
        delegate.searching_emoji(),
        "empty results never execute another action"
    );
    for ui in delegate.panels() {
        assert_eq!(
            ui.empty_labels.borrow()[0].stringValue().to_string(),
            tr!("没有匹配的表情", "No matching emoji")
        );
    }

    state.query.borrow_mut().clear();
    delegate.filter();
    let ui = delegate.panels()[0].clone();
    let editor = NSTextView::initWithFrame(NSTextView::alloc(mtm), rect(0.0, 0.0, 100.0, 30.0));
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
    assert!(delegate.searching_emoji());
    delegate.leave_scoped_search();
    assert_eq!(delegate.selected_command(), Some(CommandId::Emoji));
    assert!(state.emoji_matches.borrow().is_empty());
    for ui in delegate.panels() {
        assert!(
            !ui.rows.borrow()[0].alias.isHidden(),
            "normal row layout restored"
        );
    }
    delegate.activate_selected();
    delegate.cancel_search();
    assert!(state.mode.get().is_none());
    assert!(state.search_scope.get().is_none());
    assert!(state.emoji_matches.borrow().is_empty());
    assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));
    println!(
        "Emoji checks passed: entry, bounded rows, bilingual search, selection, target preservation, empty results, back/Escape, and no clipboard writes."
    );
}
