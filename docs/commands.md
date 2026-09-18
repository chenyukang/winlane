# Built-in commands

Winlane's search accepts windows, installed applications, and built-in commands. A matching command appears before window and application results; an empty search and switch mode never list commands. Commands require explicit selection with Enter or a mouse click. Searching only looks up metadata and never executes an action.

| Command | Search examples | Action |
| --- | --- | --- |
| `clipboard` | `clipboard`, `clip`, `剪贴板` | Open a dedicated list to search and paste clipboard history. |
| `quicklink` | `quicklinks`, `links`, `快捷链接` | Browse and open saved links. |
| `snippet` | `snippet`, `snippets`, `片段` | Open a dedicated list to browse and search saved snippets. |
| `show-menu` | `menu`, `菜单栏` | Temporarily reveal the macOS menu bar. |
| `lock-screen` | `lock`, `lock screen`, `锁屏` | Lock the Mac without closing applications. |
| `sleep` | `sleep`, `睡眠`, `休眠` | Request system sleep, not just display sleep. |
| `mission-control` | `mission control`, `mission`, `调度中心`, `窗口总览` | Open Mission Control to choose a window or desktop. |

`show-menu` reveals the macOS menu bar on the display where you selected the command. It sends a mouse-move event to that display's top edge after dismissing the picker. It leaves the pointer there so the bar remains visible until you move away. It does not open a menu, change system preferences, or restart system processes. Accessibility access is required, and macOS controls the reveal animation.

`mission-control` leaves the macOS overview open for you to select a window or desktop. It does not select a Space or perform any follow-up actions.

The picker closes before a system command executes. These commands do not change power, password, or keyboard-shortcut preferences. They do not run shell scripts or require another background process. `lock-screen` and `mission-control` resolve undocumented macOS functions at runtime, as [Hammerspoon](https://github.com/Hammerspoon/hammerspoon/tree/master/extensions) does. If an interface is missing, Winlane reports an error instead of attempting the action; future macOS changes may require an update. `sleep` uses Apple's [IOPMSleepSystem](https://developer.apple.com/documentation/iokit/1557121-iopmsleepsystem) and reports a system error if the request is rejected. The lock API does not report whether the screen actually locked.

## Add a command

1. Add a stable `CommandId` variant and a `Command` entry in `src/commands.rs`. Supply its name, English and Chinese labels, SF Symbol, and search keywords.
2. For a system action, handle the new ID in `PreparedCommand::prepare` and its execution in `src/system_commands.rs`. Commands that navigate within the picker, such as `snippet`, `clipboard`, and `quicklink`, are handled in `src/app.rs` before system dispatch. Keep specialized native operations in separate modules where needed. Preparation must not perform the action. The shared handler in `src/app.rs` dismisses the picker only after preparation succeeds and reports errors through `selection_failed`.
3. Add matching tests in `tests/commands.rs` and interaction coverage alongside `tests/support/commands.rs`. Tests should not change the user's desktop unless explicitly invoked for that purpose.

Result rendering, keyboard selection, mouse selection, and selection preservation are shared across commands. Adding a new command does not require a new result layout.

This is a compiled-in extension point. Winlane does not load third-party code or interpret search text as shell commands.
