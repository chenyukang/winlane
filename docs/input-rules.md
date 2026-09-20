# Input rules

Open **Settings → Input Rules**. This page contains a global default and per-application overrides. Rules match the app's bundle identifier, so moving or updating an app does not change its rule.

- **Default input source**: keep the current source, automatically choose an enabled English/Chinese source, or select a specific installed input source.
- **When returning to an app**: use the default each time, or restore that application's last used source. If the remembered source is unavailable, use the configured default. If the configured source is unavailable too, keep the current source.
- **Use global setting**: inherit the source and/or restore strategy independently.

The **Winlane** rule is always present and active. New installations default to English; existing installations migrate the former Winlane input preference. Choose **Keep current input source** and **Use default input source** to leave its source unchanged. The old independent preference is no longer saved or shown. All Winlane text fields continue to prepare their input source before editing, buffer early input when needed, preserve composition, and restore the original application's source on exit when appropriate. Rules do not replace that input handling.

Enable **automatic switching for other apps** to manage external apps, then use **Add App…** to choose applications. External switching starts disabled. The global default initially keeps the current source. Winlane's rule is independent of the external-switching toggle. Select a rule's **Remove** button to return that app to the global settings; the built-in Winlane rule can be edited but not removed.

Rules take effect on the next app activation. Selecting another input source manually is allowed; it is remembered without forcing the configured default back while typing. Activating another window in the same app does not reset the input source. Winlane input events are excluded from other apps' history. App rules apply to regular desktop apps, not background helpers, websites, or individual windows.

Preferences save automatically. Last-used sources are stored locally when Winlane quits normally (Winlane's existing input history remains in its existing preference key). Only app and input-source identifiers are recorded, never typed text. The external-app cache holds up to 512 apps. Unavailable input sources remain visible in Settings so they can be replaced without silently losing the rule.

The feature uses macOS application-activation and input-source notifications. It does not register new shortcuts, synthesize key presses, activate hidden windows, or add a polling timer. Automatic switching uses the standard macOS input-source selection API; third-party input methods can have their own activation behavior. Avoid configuring competing automatic switchers for the same apps.
