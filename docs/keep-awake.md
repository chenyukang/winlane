# Keep Awake

Run **keep-awake** (or **caffeine**, **防休眠**) from search. Choose **30 minutes**, **60 minutes**, or **Until turned off**, with either **Keep Mac awake; allow display sleep** or **Keep Mac and display awake**. Press Enter to apply; the picker stays open and its status shows the active session. Choosing another option replaces the previous session. Select **Turn off Keep Awake** to return to normal sleep behavior.

While enabled, an amber coffee badge stays at the top-right of each display, including full-screen Spaces. It shows **Awake** or **Display on**, with the remaining minutes or **Until off**. It does not take keyboard focus or intercept clicks, and moves below or beside the input-source indicator if they overlap. Closing the command picker leaves the badge visible; turning Keep Awake off, reaching its deadline, or quitting Winlane removes it.

The command supports a direct shortcut in **Settings → Shortcuts → Command shortcuts**. Type to filter options; Esc closes the picker without stopping an active session. Backspace on an empty input stays in the command. The back button returns to search.

Winlane uses native macOS power assertions. Timed sessions expire in the system even if Winlane's UI is busy; quitting Winlane releases all its assertions. Sessions are not restored after restarting. This prevents automatic idle sleep, not explicit Sleep commands, closing a laptop lid, or low-battery shutdown. Stopping Winlane's session does not cancel another app's sleep prevention.
