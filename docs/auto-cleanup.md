# Auto Cleanup

Auto Cleanup limits the number of windows kept open for selected apps. It is disabled by default.

1. Open **Settings → Auto Cleanup**.
2. Choose **Add App…** and select an application.
3. Set **Keep windows** to a number from 1 to 100. For example, choose Visual Studio Code and enter 3.
4. Turn on **Enable Auto Cleanup**. Settings save automatically.

The switch controls all rules. Turning it off cancels pending actions and preserves your app list and limits. A close request already delivered to an app can still finish. Remove a row to stop managing that app.

## Which windows close

Winlane checks configured apps in the background about every five seconds. It uses the same recent-window history as the switcher, independent of your display sorting preference. It closes one window at a time, starting with the least recently used eligible window. Windows with no recorded visit are considered older than windows you have used.

- The active window and windows reported as having unsaved edits are protected.
- Newly discovered windows receive at least ten seconds of grace.
- Normal windows count across all processes belonging to the same app, including minimized windows and windows on other Spaces when macOS exposes them. Tabs inside one window count as one window.
- Cleanup pauses while you use Winlane's picker or Settings, while a launch is pending, or when window information cannot be read reliably.
- Apps need working Accessibility support for their window list and close buttons. The limit is a target; protected or unavailable windows may keep an app above it.

Winlane presses the target window's normal close button. It never quits or kills the app, and never answers save or discard prompts. After a close request it waits for that window to disappear before cleaning up another window in the same app. If you cancel a close or an app rejects it, that app stays paused; turn Auto Cleanup off and on to retry. Other configured apps can continue.

## Logs

Close requests, confirmed closures, and failures are recorded even when debug logging is off:

```text
~/Library/Logs/Winlane/winlane.log
~/Library/Logs/Winlane/winlane.previous.log
```

Each file is capped at approximately 2 MiB. Entries identify the app by bundle ID and the window by numeric ID; they do not include window titles or document contents. A `close-request` means the action was sent; `closed` means WindowServer subsequently confirmed that the window disappeared. `close-unconfirmed` records a failed or uncertain operation without retrying it automatically.

Use **Settings → General → Debug logging** for additional window-discovery and recency details. **Open Logs Folder** opens the log location in Finder.
