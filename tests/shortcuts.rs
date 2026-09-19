use winlane::core::shortcuts::*;

#[test]
fn panel_navigation_handles_arrows_while_command_is_still_held() {
    use winlane::core::shortcuts::{PanelCommand, panel_command};
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
    use winlane::core::shortcuts::panel_command;
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
