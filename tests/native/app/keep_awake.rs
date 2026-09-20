use super::*;

pub(super) fn verify(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    assert!(!delegate.update_keep_awake_indicator(false));
    assert!(state.keep_awake_indicator.borrow().is_none());
    state.demo.set(true);
    state.config.borrow_mut().show_usage_hints = false;
    assert!(delegate.prepare_command_search(CommandId::KeepAwake, 100));
    assert!(delegate.searching_keep_awake());
    assert_eq!(delegate.match_count(), 7);
    assert_eq!(
        selected_choice(&delegate),
        Some(winlane::features::keep_awake::Choice::Stop)
    );
    assert!(state.matches.borrow().is_empty());
    for panel in delegate.panels() {
        assert!(!panel.footer.isHidden());
        assert!(matches!(
            panel.rows.borrow()[0].content,
            Some(RowContent::KeepAwake(_))
        ));
        assert!(!panel.panel.isVisible());
    }
    state.query.replace("60".into());
    delegate.filter();
    assert_eq!(delegate.match_count(), 2);
    let selected = delegate.selected_result();
    delegate.filter_preserving(selected);
    assert_eq!(state.selected.get(), 0);
    state.query.replace("unknown".into());
    delegate.filter();
    delegate.activate_selected();
    assert_eq!(delegate.match_count(), 0);
    state.query.replace(String::new());
    delegate.filter();
    state.demo.set(false);
    state.catalog_checked.set(Some(Instant::now()));
    delegate.leave_scoped_search();
    assert!(!delegate.searching_keep_awake());
    assert_eq!(delegate.selected_command(), Some(CommandId::KeepAwake));
    delegate.enter_scoped_search(SearchScope::KeepAwake);
    delegate.end_session();
    assert!(state.keep_awake_matches.borrow().is_empty());
    assert!(state.mode.get().is_none());
}

pub(super) fn verify_reopened_selection(mtm: MainThreadMarker) {
    use winlane::features::keep_awake::{CHOICES, Choice};
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.demo.set(true);
    for choice in CHOICES {
        state.keep_awake.borrow_mut().apply(choice).unwrap();
        assert!(delegate.prepare_command_search(CommandId::KeepAwake, 200));
        assert_eq!(selected_choice(&delegate), Some(choice));
        // Moving the highlight without applying must not replace the active choice.
        state
            .selected
            .set((state.selected.get() + 1) % CHOICES.len());
        delegate.end_session();
        assert!(delegate.prepare_command_search(CommandId::KeepAwake, 201));
        assert_eq!(selected_choice(&delegate), Some(choice));
    }
    let active = Choice::Start {
        minutes: Some(60),
        display: true,
    };
    state.keep_awake.borrow_mut().apply(active).unwrap();
    state.query.replace("60".into());
    delegate.filter();
    assert_eq!(state.selected.get(), 1);
    let manual = Choice::Start {
        minutes: Some(60),
        display: false,
    };
    delegate.filter_preserving(Some(SelectedResult::KeepAwake(manual)));
    assert_eq!(selected_choice(&delegate), Some(manual));
    state.query.replace("30".into());
    delegate.filter();
    assert_eq!(state.selected.get(), 0);
    state.query.borrow_mut().clear();
    delegate.filter();
    assert_eq!(selected_choice(&delegate), Some(active));
    state
        .keep_awake
        .borrow_mut()
        .expire(std::time::SystemTime::now() + Duration::from_secs(7200));
    delegate.end_session();
    delegate.prepare_command_search(CommandId::KeepAwake, 202);
    assert_eq!(selected_choice(&delegate), Some(Choice::Stop));
    assert!(state.keep_awake_indicator.borrow().is_none());
    assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));
}

fn selected_choice(delegate: &Delegate) -> Option<winlane::features::keep_awake::Choice> {
    match delegate.selected_result() {
        Some(SelectedResult::KeepAwake(choice)) => Some(choice),
        _ => None,
    }
}
