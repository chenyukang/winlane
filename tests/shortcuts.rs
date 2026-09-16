use winlane::shortcuts::*;

#[test]
fn command_tab_is_consumed_and_shift_reverses_selection() {
    let mut state = CommandTabState::default();
    assert_eq!(
        state.handle(KEY_DOWN, TAB, COMMAND, false),
        (true, Some(CommandTabAction::Next))
    );
    assert_eq!(state.handle(KEY_UP, TAB, COMMAND, false), (true, None));
    assert_eq!(
        state.handle(KEY_DOWN, TAB, COMMAND | SHIFT, false),
        (true, Some(CommandTabAction::Previous))
    );
}

#[test]
fn repeated_press_does_not_toggle_or_skip_multiple_items() {
    let mut state = CommandTabState::default();
    state.handle(KEY_DOWN, TAB, COMMAND, false);
    assert_eq!(state.handle(KEY_DOWN, TAB, COMMAND, true), (true, None));
    assert_eq!(state.handle(KEY_DOWN, TAB, COMMAND, false), (true, None));
    state.handle(KEY_UP, TAB, COMMAND, false);
    assert_eq!(
        state.handle(KEY_DOWN, TAB, COMMAND, false).1,
        Some(CommandTabAction::Next)
    );
    let mut latch = PressLatch::default();
    assert!(latch.update(true));
    assert!(!latch.update(true));
    assert!(!latch.update(false));
    assert!(latch.update(true));
}

#[test]
fn releasing_command_before_tab_does_not_leak_the_consumed_key_up() {
    let mut state = CommandTabState::default();
    state.handle(KEY_DOWN, TAB, COMMAND, false);
    assert_eq!(state.handle(FLAGS_CHANGED, 55, 0, false), (false, None));
    assert_eq!(state.handle(KEY_UP, TAB, 0, false), (true, None));
    assert_eq!(state.handle(KEY_DOWN, TAB, 0, false), (false, None));
    assert_eq!(state.handle(KEY_UP, TAB, 0, false), (false, None));
}

#[test]
fn unrelated_shortcuts_and_normal_typing_pass_through() {
    let mut state = CommandTabState::default();
    for flags in [
        0,
        SHIFT,
        OPTION,
        CONTROL,
        COMMAND | CONTROL,
        COMMAND | OPTION,
    ] {
        assert_eq!(state.handle(KEY_DOWN, TAB, flags, false), (false, None));
        assert_eq!(state.handle(KEY_UP, TAB, flags, false), (false, None));
    }
    for key in [0, 12, 36, ESCAPE] {
        assert_eq!(state.handle(KEY_DOWN, key, COMMAND, false), (false, None));
    }
    assert_eq!(
        state.handle(KEY_DOWN, TAB, COMMAND | (1 << 16), false).1,
        Some(CommandTabAction::Next)
    );
}

#[test]
fn escape_cancels_only_the_active_command_tab_session() {
    let mut state = CommandTabState::default();
    state.handle(KEY_DOWN, TAB, COMMAND, false);
    state.handle(KEY_UP, TAB, COMMAND, false);
    assert_eq!(
        state.handle(KEY_DOWN, ESCAPE, COMMAND, false),
        (true, Some(CommandTabAction::Cancel))
    );
    assert_eq!(state.handle(KEY_DOWN, ESCAPE, COMMAND, true), (true, None));
    assert_eq!(state.handle(KEY_UP, ESCAPE, 0, false), (true, None));
    assert_eq!(
        state.handle(KEY_DOWN, ESCAPE, COMMAND, false),
        (false, None)
    );
}

#[test]
fn panel_navigation_handles_arrows_while_command_is_still_held() {
    use winlane::shortcuts::{PanelCommand, panel_command};
    for flags in [0, COMMAND] {
        assert_eq!(panel_command(125, flags, false), Some(PanelCommand::Next));
        assert_eq!(
            panel_command(126, flags, false),
            Some(PanelCommand::Previous)
        );
    }
    assert_eq!(panel_command(TAB, 0, false), Some(PanelCommand::Next));
    assert_eq!(
        panel_command(TAB, SHIFT, false),
        Some(PanelCommand::Previous)
    );
    assert_eq!(panel_command(36, 0, false), Some(PanelCommand::Accept));
    assert_eq!(panel_command(ESCAPE, 0, false), Some(PanelCommand::Cancel));
}

#[test]
fn panel_navigation_preserves_composition_text_editing_and_system_shortcuts() {
    use winlane::shortcuts::panel_command;
    for key in [125, 126, TAB, 36, ESCAPE] {
        assert_eq!(panel_command(key, 0, true), None, "IME key {key}");
        assert_eq!(panel_command(key, CONTROL, false), None);
        assert_eq!(panel_command(key, OPTION, false), None);
    }
    for (key, flags) in [
        (0, 0),
        (8, COMMAND),
        (51, 0),
        (123, 0),
        (124, 0),
        (TAB, COMMAND),
        (36, COMMAND),
        (125, SHIFT),
    ] {
        assert_eq!(panel_command(key, flags, false), None);
    }
}
