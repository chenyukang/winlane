# Keep Awake

Run **keep-awake** (or **caffeine**, **防休眠**) from search. Choose **30 minutes**, **60 minutes**, or **Until turned off**, with either **Keep Mac awake; allow display sleep** or **Keep Mac and display awake**. Press Enter to apply; the picker stays open and its status shows the active session. Choosing another option replaces the previous session. Select **Turn off Keep Awake** to return to normal sleep behavior.

While enabled, an amber coffee badge stays at the top-right of each display, including full-screen Spaces. It shows **Awake** or **Display on**, with the remaining minutes or **Until off**. It does not take keyboard focus or intercept clicks, and moves below or beside the input-source indicator if they overlap. Closing the command picker leaves the badge visible; turning Keep Awake off, reaching its deadline, or quitting Winlane removes it.

The command supports a direct shortcut in **Settings → Shortcuts → Command shortcuts**. Reopening the picker selects the currently active duration and display mode, or **Turn off Keep Awake** if the session is off or expired. Reopening does not restart the timer. Type to filter options; Esc closes the picker without stopping an active session. Backspace on an empty input stays in the command. The back button returns to search.

Winlane uses native macOS power assertions. Timed sessions expire in the system even if Winlane's UI is busy; quitting Winlane releases all its assertions. Sessions are not restored after restarting. This prevents automatic idle sleep, not explicit Sleep commands, closing a laptop lid, or low-battery shutdown. Stopping Winlane's session does not cancel another app's sleep prevention.

## Which apps keep the Mac awake

Winlane can hold a power assertion back only for itself. Any other application
can hold one of its own, and then the display never turns off and the Mac never
idles to sleep, no matter what Keep Awake is set to.

While the Keep Awake panel or the Settings window is open, Winlane reads the
machine's power assertions on a background thread every five seconds. The
Keep Awake footer, and the Auto AppClose page, name the applications that hold
sleep back, for example `微信 正在阻止休眠`. System processes are left out: the
power manager's own "prevent sleep while the display is on" assertion is a
consequence of another app's display assertion, not a cause.

When a blocking app appears while Settings is open, Winlane asks once whether to
add an Auto AppClose rule that closes that app's windows after 60 idle minutes.

- **Add idle closing** adds the rule for each named app that does not have one
  yet, and turns Auto AppClose on, because a rule does nothing while the switch
  is off. The rules appear in **Settings → Auto AppClose**, where the idle time
  can be changed.
- **Leave it as is** changes nothing. The note stays in Settings, so the app can
  also be handled by hand, for example by closing the video window that holds
  the wake lock.

The assertions are read with `pmset -g assertions`, which reports state and
changes nothing. Winlane never releases another application's assertion.
