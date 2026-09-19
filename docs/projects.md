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

The panel opens immediately with the last in-memory results, or a loading message on the first visit. VS Code discovery and history reads run in the background after the panel is prepared. Each visit checks for changes to the source files and SQLite write-ahead log; **Command + R** forces a reload. You can keep typing or selecting cached projects during a refresh, and the updated list preserves your query and selected project.

Leaving the list keeps its cache and lets any in-flight read finish for the next visit. A failed read keeps the previous results and shows an error. The cache lasts until Winlane quits. Winlane does not scan project folders, poll VS Code continuously, modify its history, or save another copy to disk.

This version supports local projects in the stable VS Code app's default data location. Remote SSH, WSL and container projects, VS Code Insiders, custom `--user-data-dir` locations, and individual recently opened files are not included.
