# Built-in commands

Winlane's search accepts windows, installed applications, and built-in commands. A matching command appears before window and application results; an empty search and switch mode never list commands. Commands require explicit selection with Enter or a mouse click. Searching only looks up metadata and never executes an action.

`show-menu` reveals the macOS menu bar on the display where you selected the command. It sends a mouse-move event to that display's top edge after dismissing the picker. It leaves the pointer there so the bar remains visible until you move away. It does not open a menu, change system preferences, or restart system processes. Accessibility access is required, and macOS controls the reveal animation.

## Add a command

1. Add a stable `CommandId` variant and a `Command` entry in `src/commands.rs`. Supply its name, English and Chinese labels, SF Symbol, and search keywords.
2. Handle the new ID in `Delegate::execute_command` in `src/app.rs`. Keep native operations in a separate module, as `src/menu_bar.rs` does. Validate the action before dismissing the picker, and report errors through `selection_failed`.
3. Add matching tests in `tests/commands.rs` and interaction coverage alongside `tests/support/commands.rs`. Tests should not change the user's desktop unless explicitly invoked for that purpose.

Result rendering, keyboard selection, mouse selection, and selection preservation are shared across commands. Adding a new command does not require a new result layout.

This is a compiled-in extension point. Winlane does not load third-party code or interpret search text as shell commands.
