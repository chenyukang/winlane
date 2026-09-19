# Recent VS Code projects

Open search with **Control + I**, type **projects** or **vscode**, and select **Open recent VS Code projects**. The same search panel becomes a project list; projects stay out of ordinary window results.

- Type part of a project name or path to filter the list. Multiple words must all match, ignoring case.
- Use the arrow keys or mouse to select a project. Press **Enter** to open it in Visual Studio Code, launching the app if necessary.
- Press **Command + R** to reload the history.
- Press **Esc** to close the picker. Click **‹ Projects** to return to window search. Backspace only deletes text; an empty search stays in Projects.

Each row shows the project name and its path. Folders and saved `.code-workspace` files are supported. Opening follows VS Code's own window-opening preferences. A deleted, moved, or unmounted project reports an error and leaves the list open.

After sending the open request, Winlane waits in the background for the target project window to appear, then restores it if minimized, raises it, and activates VS Code. Ordinary and fullscreen windows keep their existing layout. Window identification uses the project name and any document path exposed by VS Code; it does not pick the first Code window. The wait is limited to eight seconds. If custom titles or ambiguous same-named projects prevent identification, Winlane only activates the app. Reopening Winlane or switching to another app cancels the pending focus action.

## History and performance

Winlane reads the stable Visual Studio Code app's existing history locally. It prefers the shared history database, whose folder name comes from VS Code's `product.json` (normally `~/.vscode-shared/sharedStorage/state.vscdb`), and falls back to `~/Library/Application Support/Code/User/globalStorage/state.vscdb`. Both the newer `recently.opened` and older `history.recentlyOpenedPathsList` records are supported.

The history's most-recent-first order is preserved when filtering. Backup and profile metadata from `globalStorage/storage.json` can supply additional projects; these appear after recorded recent projects because that metadata does not provide reliable recency. Duplicate paths are removed, with a limit of 500 projects. The list displays up to 25 matches at a time; typing searches all loaded projects, including those outside the initial 25.

The panel opens with cached results, or a loading message if none are available. A small spinner indicates a pending refresh without blocking input or cached selections. Creating at most 25 result rows per display keeps a large history from delaying the first frame. Each visit checks for changes to the source files and SQLite write-ahead log in the background; **Command + R** forces a reload. The updated list preserves your query and selected project when it remains among the matching results.

The 25 most recent project names, paths, and folder/workspace types are saved locally at `~/Library/Caches/app.windowlane.desktop/projects.json`. Startup restores this small snapshot on a worker thread before refreshing VS Code history, so a restart does not require an empty first visit. Snapshot reads, app discovery, database reads, and cache writes all run off the UI thread. Missing or corrupt snapshots fall back to fresh discovery.

Leaving the list keeps its in-memory cache and lets any in-flight read finish for the next visit. A failed read retains the previous results; a successful empty history clears the saved snapshot. Winlane does not scan project folders, poll VS Code continuously, or modify its history. The disposable snapshot is written atomically with access limited to the current user.

This version supports local projects in the stable VS Code app's default data location. Remote SSH, WSL and container projects, VS Code Insiders, custom `--user-data-dir` locations, and individual recently opened files are not included.
