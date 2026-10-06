# Auto AppClose

Auto AppClose closes windows you no longer need for selected apps. A rule can cap how many windows an app keeps, close windows that sit unused, or do both. It is disabled by default.

1. Open **Settings → Auto AppClose**.
2. Choose **Add App…** and select an application.
3. Set **Keep windows** to a number from 1 to 100, and/or **Idle (minutes)** to how long a window may go unused, 1 to 10080 (one week). Leave a field empty to leave that limit out. For example, keep Visual Studio Code to 3 windows, or close Finder windows four hours after you last used them with an idle time of 240.
4. Set **Check interval** in seconds (1–3600). The default is **10 seconds**; the new-window grace period is always twice that interval.
5. Turn on **Enable Auto AppClose**. Settings save automatically.

Every rule needs at least one of the two limits; a row with both fields empty is not saved. The switch controls all rules. Turning it off cancels pending actions and preserves your app list and limits. A close request already delivered to an app can still finish. Remove a row to stop managing that app.

## Which windows close

Winlane checks configured apps in the background at your chosen interval, every ten seconds by default. Changes take effect without restarting Winlane. It uses the same recent-window history as the switcher, independent of your display sorting preference. It closes one window at a time, starting with the least recently used eligible window, and waits for that window to disappear before closing another in the same app.

- **Keep windows**: while the app holds more windows than the limit, the least recently used eligible window closes. Windows with no recorded visit are considered older than windows you have used.
- **Idle (minutes)**: a window that has gone unused for longer than the limit closes, and the window unused the longest goes first. Idle time counts from the last visit Winlane recorded for that window, so using a window restarts its clock even between two checks. A window that was already open when Winlane started counts from the first scan that saw it, so enabling a rule never closes a whole session at once.
- The active window and windows reported as having unsaved edits are protected, and either limit re-reads the app just before closing: the window must still be there, unmodified, and not the one in use.
- Newly discovered windows receive at least twice the check interval as grace: 20 seconds with the default interval, or 60 seconds if you choose a 30-second interval.
- Normal windows count across all processes belonging to the same app, including minimized windows and windows on other Spaces when macOS exposes them. Tabs inside one window count as one window.
- Auto AppClose pauses while you use Winlane's picker or Settings, while a launch is pending, or when window information cannot be read reliably.
- Apps need working Accessibility support for their window list and close buttons. A limit is a target; protected or unavailable windows may keep an app above it.

Winlane presses the target window's normal close button. It never quits or kills the app, and never answers save or discard prompts. After a close request it waits for that window to disappear before closing another window in the same app. If you cancel a close or an app rejects it, that app stays paused; turn Auto AppClose off and on to retry. Other configured apps can continue.

## Logs

At the default Info level, close requests, confirmed closures, and failures are recorded. The default log files are:

```text
~/Library/Logs/Winlane/winlane.log
~/Library/Logs/Winlane/winlane.previous.log
```

Each file is capped at approximately 2 MiB. Entries identify the app by bundle ID and the window by numeric ID; they do not include window titles or document contents. Each entry names the limit that applied: `reason=WindowLimit keep=3` or `reason=Idle idle_minutes=240`. A `close-request` means the action was sent; `closed` means WindowServer subsequently confirmed that the window disappeared. `close-unconfirmed` records a failed or uncertain operation without retrying it automatically.

Use **Settings → General → Logging** to change the level or log file path. Debug adds window-discovery and recency details; Warn includes Auto AppClose failures but omits normal close events. Error and Off omit these Auto AppClose entries. **Open Logs Folder** opens the configured location in Finder.
