# Recent VS Code projects

Open search with **Control + I**, type **projects** or **vscode**, and select **Open recent VS Code projects**. The same search panel becomes a project list; projects stay out of ordinary window results.

- Type part of a project name or path to filter the list. Multiple words must all match, ignoring case.
- Use the arrow keys or mouse to select a project. Press **Enter** to open it in Visual Studio Code, launching the app if necessary.
- Press **Command + R** to reload the history.
- Press **Esc**, click **‹ Projects**, or press Backspace in an empty search field to return to window search.

Each row shows the project name and its path. Folders and saved `.code-workspace` files are supported. Opening follows VS Code's own window-opening preferences. A deleted, moved, or unmounted project reports an error and leaves the list open.

## History and performance

Winlane reads the stable Visual Studio Code app's existing history locally. It prefers the shared history database, whose folder name comes from VS Code's `product.json` (normally `~/.vscode-shared/sharedStorage/state.vscdb`), and falls back to `~/Library/Application Support/Code/User/globalStorage/state.vscdb`. Both the newer `recently.opened` and older `history.recentlyOpenedPathsList` records are supported.

The history's most-recent-first order is preserved when filtering. Backup and profile metadata from `globalStorage/storage.json` can supply additional projects; these appear after recorded recent projects because that metadata does not provide reliable recency. Duplicate paths are removed, with a limit of 500 projects.

History loads on a background thread when you enter the project list or request a refresh. Subsequent visits reuse an in-memory cache if the source files and SQLite write-ahead log have not changed. Typing searches only the loaded list. Winlane does not scan project folders, poll VS Code continuously, modify its history, or save another copy to disk.

This version supports local projects in the stable VS Code app's default data location. Remote SSH, WSL and container projects, VS Code Insiders, custom `--user-data-dir` locations, and individual recently opened files are not included.
