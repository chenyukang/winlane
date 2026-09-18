# Snippets

Open **Settings → Snippets**, choose **New**, and enter a name and some text. Valid changes save automatically on this Mac. Incomplete or invalid edits stay as drafts in the editor; the previous saved version remains available until the draft is valid. Drafts are discarded when Winlane quits. Delete removes the selected snippet. Restoring default settings keeps your snippets.

Open search with **Control + I**, type **snippet**, and select the **Search snippets** command. This opens a dedicated snippet list: an empty query shows all saved snippets, and typing filters by name or content. Results marked **{}** paste a snippet. Full name matches come before partial names and content matches.

The **‹ Snippets** button returns to ordinary search. You can also press **Esc**, or Backspace when the query is empty; another Esc closes the picker. Spaces stay part of the query in snippet search. Ordinary search and switch mode do not mix in snippets, so window aliases keep their usual behavior. Entering snippet search preserves the application you were using as the paste destination.

Selecting a snippet pastes plain text into the application that was active before Winlane opened. If it contains input fields, fill them in, review the preview, then choose **Paste** or press Enter. Forms with multiline fields use **Command + Enter** to paste, leaving Enter available for line breaks. Press Esc to cancel. Winlane waits for the destination app to gain focus and for modifier keys to be released; if another app takes focus, it cancels the paste. The expanded text stays on your clipboard. This requires Accessibility permission and a destination that accepts Command + V.

## Placeholders

Choose **Insert Placeholder** in the editor. Configuration stays inside the same Settings window, and the placeholder is inserted at your text selection only after you choose **Insert**. Cancel leaves the snippet unchanged.

- **Date & Time** offers system date, `yyyy-MM-dd`, Chinese date, date and time, 24-hour time, weekday, and Unix timestamp presets. Each preset shows an example. Choose **Custom** to enter a format and see its preview.
- **Custom Input Field** lets you set a name, type (**Text**, **Multiline**, or **Dropdown**), an optional default value, and whether it is required. Enter dropdown options one per line; its default must match an option. Use **New / reuse existing field** to insert another occurrence with the same configuration.
- **Clipboard** inserts the clipboard placeholder directly.

You can also type placeholders directly:

| Placeholder | Value |
| --- | --- |
| `{clipboard}` | Plain text copied before selecting the snippet; empty if unavailable |
| `{date}` | Current date in the system's regional format |
| `{time}` | Current time in the system's regional format |
| `{datetime}` | Current date and time |
| `{date format="yyyy-MM-dd"}` | Custom date format; `time` and `datetime` also accept `format` |
| `{timestamp}` | Unix timestamp in seconds |
| `{argument name="Name"}` | A required input field |
| `{argument name="Tone" default="friendly"}` | A field with a default value |
| `{argument name="Notes" type="multiline"}` | A required multiline field |
| `{argument name="Extra" required="false"}` | An optional field that may be left empty |
| `{argument name="Tone" type="choice" options="friendly\nformal" default="friendly"}` | A dropdown field |

Dates are evaluated when you confirm the paste. Repeated arguments with the same name share one field and must use the same type, default, required setting, and options. You can use up to eight distinct fields in a snippet. For example:

```text
Hi {argument name="Name"},

Here are the notes from {date format="yyyy-MM-dd"}:

{clipboard}

Thanks, {argument name="Name"}!
```

Existing placeholders continue to work: fields without a default are required, and fields with a default are optional unless `required="true"` is specified. The form starts with defaults filled in; you can clear an optional field. Required fields and dropdown values are checked before pasting. Quote and backslash characters inside attributes can be escaped as `\"` and `\\`; `\n` inserts a newline in multiline defaults and separates dropdown options.

The preview in the editor uses sample values; the input form shows the captured clipboard and your actual field values. Placeholders inside clipboard text or field values are not expanded again.

Use `\{date}` to paste the literal text `{date}`. Ordinary braces in code and unknown placeholder names are preserved. Supported placeholders with invalid parameters show a validation error. Snippets currently support plain text and the placeholders listed above, rather than Raycast's entire placeholder and rich-text feature set.

Up to 200 snippets can be saved. Names must be unique and at most 120 characters; each template is limited to 64 KB and expanded text to 1 MB. Snippets are stored with the other preferences in the `WindowlanePreferencesV1` key of the `app.windowlane.desktop` defaults domain. Clipboard values and filled-in arguments are not saved in templates.
