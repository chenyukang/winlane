# Open URL

Search for **open-url** (or **浏览记录**) and select **Open URL or search Google**. This opens a dedicated list, like `projects`; browsing history never appears in ordinary window search.

To open it directly from another app, configure `open-url` under **Settings → Shortcuts → Command shortcuts**. Changes save immediately; **Not set** removes the shortcut.

- Pages are ordered by their most recent visit. Repeated URLs appear once.
- Search by page title, domain, or URL. Multiple words narrow the results while keeping their recent order.
- With an empty search, Enter opens the selected history item. While typing, Enter uses your input: an HTTP/HTTPS URL, domain, IP address, or `localhost` opens directly; other text becomes a Google search. Addresses without a scheme use HTTPS (`localhost`, `127.0.0.1` and `::1` use HTTP). Both open in a new Chrome tab, including when history has no match or cannot be read.
- Press Down or Tab to select a history item, then Enter to open that item; clicking a row also opens it. Press Up from the first result to return to the input action. Editing the query returns selection to the input, and background refreshes preserve an explicitly selected history item. Chrome chooses the destination window/profile using its normal URL handling.
- **Command + R** reads the history again. Esc, the back button, or Backspace in an empty search returns to `open-url` in ordinary search.

Winlane combines the `Default` and numbered `Profile N` histories under `~/Library/Application Support/Google/Chrome`. It reads the latest 1,000 distinct HTTP/HTTPS URLs across up to 32 profiles and displays up to 100 matching results; type more words to narrow the list. It does not include guest profiles, private browsing, internal browser pages, or local files. Custom Chrome data-directory locations and other browsers are not supported yet.

The panel opens immediately with the last in-memory results, or a loading message on the first visit. A background worker refreshes history after entry, so you can type or select cached results while it loads. Refreshes preserve your query and explicitly selected URL. Chrome may keep its history database locked. Winlane reads a temporary private copy of `History` together with its SQLite write-ahead/rollback journal, checks for changes during copying, and lets SQLite recover committed data in that copy. It never writes to Chrome's files. The copy is deleted when the read finishes. Individual source files larger than 256 MB are skipped with an error; other readable profiles still appear.

Up to 1,000 results stay cached in memory until Winlane quits. Leaving the list clears its visible rows but lets an in-flight read finish and update the cache for the next visit. A failed read keeps the previous results and shows an error; a successful empty read clears the cache. No separate history library is saved, and no webpage or favicon is fetched during search. Chrome may take a moment to flush its latest visit to disk.

macOS may block Winlane from reading Chrome's data. When access is denied, the list shows **Open Access Settings…**, even when usage hints are hidden. Open **System Settings → Privacy & Security → Full Disk Access**, add `/Applications/Winlane.app` if necessary, and enable it. Restart Winlane after granting access, then run `open-url` again. This permission belongs to the installed app; a successful read from Terminal or a test binary does not grant Winlane access. Full Disk Access is a broad system permission, described in [Apple's privacy settings guide](https://support.apple.com/guide/mac-help/mchl211c911f/mac).

Other read failures show an error without suggesting a permission change. Use **Command + R** to retry. If only some profiles are inaccessible, readable profiles still appear.

The reader uses Chromium's [`urls` schema](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/components/history/core/browser/url_database.cc) and SQLite's [journal recovery](https://www.sqlite.org/lockingv3.html#dealing_with_hot_journals).
