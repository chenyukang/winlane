# Built-in commands

Winlane's search accepts windows, installed applications, and built-in commands. A matching command appears before window and application results; an empty search and switch mode never list commands. Select a command with Enter or a mouse click, or assign it a global shortcut. Searching only looks up metadata and never executes an action.

In **Settings → Shortcuts → Command shortcuts**, find the command, select its modifiers and key, and the shortcut takes effect immediately. Choose **Not set** to remove it. Commands start without an assigned shortcut. Conflicts with search, switch (including reverse switching), apps, Quicklinks, and other commands are rejected without saving.

For example, binding **Command + Option + U** to `open-url` opens its URL/search input directly. `projects`, `clipboard`, `snippet`, and `quicklink` also open their own lists. System commands such as `lock-screen` and `sleep` execute immediately when their configured shortcut is pressed. Holding the shortcut does not repeat the command.

You can also bind a memorable **alias** to a command in **Settings → Aliases**: add a rule, choose **Command** as its type, pick the command, and enter one or two lowercase letters. Typing that alias in search lists the command first, the same way an app alias surfaces an app. An alias cannot be shared between an app and a command, and command aliases ignore the title filter.

| Command | Search examples | Action |
| --- | --- | --- |
| `files` | `files`, `finder`, `documents`, `文件` | Search filenames, browse a path, open, preview, or reveal in Finder. [Guide](files.md). |
| `open-url` | `open-url`, `history`, `浏览记录`, `最近网址` | Browse Chrome history, open a typed URL, or search Google in Chrome. |
| `projects` | `projects`, `vscode`, `vs code`, `最近项目`, `项目` | Browse recent local VS Code folders and workspaces, then open a project. |
| `branch` | `branch`, `git branch`, `checkout`, `分支`, `切换分支` | Switch the current VS Code project branch. [Guide](git-branch.md). |
| `clipboard` | `clipboard`, `clip`, `剪贴板` | Open a dedicated list to search and paste clipboard history. |
| `quicklink` | `quicklinks`, `links`, `快捷链接` | Browse and open saved links. |
| `snippet` | `snippet`, `snippets`, `片段` | Open a dedicated list to browse and search saved snippets. |
| `emoji` | `emoji`, `emojis`, `表情` | Search emoji by English or Chinese names and paste into the previous app. |
| `show-menu` | `menu`, `菜单栏` | Temporarily reveal the macOS menu bar. |
| `lock-screen` | `lock`, `lock screen`, `锁屏` | Lock the Mac without closing applications. |
| `screenshot` | `screenshot`, `screen shot`, `capture area`, `截图`, `截屏` | Select an area with the mouse and copy its screenshot to the clipboard. |
| `toggle-appearance` | `dark mode`, `light mode`, `theme`, `切换外观`, `暗黑模式` | Toggle macOS between light and dark appearance. |
| `sleep` | `sleep`, `睡眠`, `休眠` | Request system sleep, not just display sleep. |
| `mission-control` | `mission control`, `mission`, `调度中心`, `窗口总览` | Open Mission Control to choose a window or desktop. |
| `date` | `date`, `time`, `clock`, `日期`, `时间` | Show the current date and time and copy it to the clipboard. |
| `meeting` | `meeting`, `calendar`, `zoom`, `会议`, `日程` | List today’s upcoming calendar meetings and open a meeting link. [Guide](meeting.md). |

`show-menu` reveals the macOS menu bar on the display where you selected the command. It sends a mouse-move event to that display's top edge after dismissing the picker. It leaves the pointer there so the bar remains visible until you move away. It does not open a menu, change system preferences, or restart system processes. Accessibility access is required, and macOS controls the reveal animation.

`mission-control` leaves the macOS overview open for you to select a window or desktop. It does not select a Space or perform any follow-up actions.

`date` shows the current local date and time as a search result, formatted like `2026-9-23 18:06:55` with an unpadded month and day. The row reflects the moment it was last drawn as you type. Selecting it copies the shown timestamp to the clipboard and closes the picker; it does not paste into another app.

`emoji` opens a dedicated chooser with common emoji first. Search English or
Chinese names and keywords, or paste an emoji to find that exact sequence. Names
with underscores, such as `:thumbs_up:`, also work. Up to 100 matches are shown;
refine the search to find more. Enter or a mouse click pastes the complete emoji,
including any skin tone or joined characters, into the app active before Winlane
opened. This uses the same focus checks and paste mechanism as snippets and
requires Accessibility access. Esc closes the chooser, and empty Backspace stays
in it. The [catalog](../resources/emoji/README.md) is embedded in the executable;
no download is needed at runtime.

`screenshot` closes Winlane's picker and starts macOS's interactive area capture. Drag to select the area you want; releasing the mouse copies the image to the clipboard for pasting with **Command + V**. Press **Esc** to cancel without replacing the clipboard. It does not take an automatic full-screen capture or save a file. When clipboard history is enabled, the image follows its usual recording and retention rules.

The screenshot command runs `/usr/sbin/screencapture -i -s -c -d` without a shell. It waits for your selection outside Winlane's main thread and lets macOS display capture errors. The helper exits when you finish or cancel; merely searching for the command does not start it.

`toggle-appearance` reads the current system appearance when you execute it, then switches light to dark or dark to light. Both `dark mode` and `light mode` find the same toggle command. Winlane's own appearance preference is preserved: choose **Settings → Appearance → System** if you want Winlane to follow the system change.

The appearance toggle resolves SkyLight's native [system appearance functions](https://saagarjha.com/blog/2018/12/01/scheduling-dark-mode/) at runtime. It does not launch a helper or send Apple Events to System Events. If either function is unavailable, the command reports an error without changing the appearance. These are undocumented interfaces and may need updating for future macOS versions; the setter does not report whether every application has finished updating.

The picker closes before a system command executes. These commands do not change power, password, or keyboard-shortcut preferences. They do not run shell scripts or require a persistent helper. `lock-screen` and `mission-control` resolve undocumented macOS functions at runtime, as [Hammerspoon](https://github.com/Hammerspoon/hammerspoon/tree/master/extensions) does. If an interface is missing, Winlane reports an error instead of attempting the action; future macOS changes may require an update. `sleep` uses Apple's [IOPMSleepSystem](https://developer.apple.com/documentation/iokit/1557121-iopmsleepsystem) and reports a system error if the request is rejected. The lock API does not report whether the screen actually locked.

## Add a command

1. Add a stable `CommandId` variant and a `Command` entry in `src/core/commands.rs`. Supply its name, English and Chinese labels, SF Symbol, and search keywords.
2. For a system action, handle the new ID in `PreparedCommand::prepare` and its execution in `src/macos/platform/system_commands.rs`. Commands that navigate within the picker, such as `snippet`, `clipboard`, `quicklink`, and `projects`, are handled in `src/macos/app/commands.rs` before system dispatch. Keep specialized native operations in separate modules where needed. Preparation must not perform the action. The shared handler in `src/macos/app/commands.rs` dismisses the picker only after preparation succeeds and reports errors through `selection_failed` in `src/macos/app/windows.rs`.
3. Add matching tests in `tests/commands.rs` and interaction coverage in `tests/native/app/commands.rs`. Tests should not change the user's desktop unless explicitly invoked for that purpose.

Result rendering, keyboard selection, mouse selection, and selection preservation are shared across commands. Adding a new command does not require a new result layout.

This is a compiled-in extension point. Winlane does not load third-party code or interpret search text as shell commands.

`branch` looks up the current VS Code project and lists its local git branches. Enter switches to the selected branch with `git switch`; if another worktree already has that branch checked out, Git reports the conflict. Winlane does not auto-commit dirty changes.

`keep-awake` opens a duration picker for preventing idle sleep, with an optional always-on display. See [Keep Awake](keep-awake.md).
