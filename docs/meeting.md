# Meetings

Run `meeting` (or search `calendar`, `会议`, `日程`) to list today's upcoming events from the macOS Calendar and open a meeting link. The list opens in its own scoped view, shows cached rows or a loading state immediately, and reads events in the background.

Each row shows the start time, how soon the meeting starts (for example, **Starting in 20 minutes**), the event title, and the detected join link or location. Press **Enter** to open the selected meeting's link in your default browser. Type to filter by title, calendar, or link. **Command + R** refreshes, and **Esc** or the back button returns to the main search.

Press **`>`** to move to the next day and **`<`** to move to the previous day; the footer shows which day you are viewing (Today, Tomorrow, Yesterday, and so on). Return to Today by pressing **`<`**/**`>`** back to it.

## What is shown

- On **Today**, only events that have not yet ended, from now until midnight, sorted by start time. All-day events are skipped.
- On **other days**, the whole day is shown, including meetings that already finished. Each finished meeting is marked **Completed**; an ongoing one is marked **In progress**, and upcoming ones show a countdown.
- Meeting links are taken from the event's URL first, then a recognized provider link in its location or notes. Recognized providers include Zoom, Google Meet, Microsoft Teams, Webex, Whereby, Jitsi, BlueJeans, Chime, GoToMeeting, RingCentral, Tencent Meeting, and Feishu/Lark.
- Events without a link still appear so you can see the schedule; selecting one reports that it has no link to open.

## Calendar access

Winlane reads events through EventKit and requests Calendar access on first use. If access is denied, the list explains how to enable it in **System Settings → Privacy & Security → Calendars**; grant access and press **Command + R**. Events from Google, Exchange, or iCloud accounts appear only if those accounts are added to the macOS Calendar app.

Winlane only reads events to display them. It does not create, edit, or delete calendar data, and it opens links in your default browser without sending event details anywhere.
