use super::*;

pub(super) fn verify(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.demo.set(true);
    state.config.borrow_mut().show_usage_hints = false;
    assert!(delegate.prepare_command_search(CommandId::KeepAwake, 100));
    assert!(delegate.searching_keep_awake());
    assert_eq!(delegate.match_count(), 7);
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
