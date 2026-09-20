# Find files

Run **`files`** from search, or assign it a shortcut in **Settings → Shortcuts → Command shortcuts**. Files have their own list and do not crowd ordinary window and app results.

The panel opens immediately. An empty search shows up to 25 files and folders you opened through Winlane, including after a restart. If there are no recent items, it offers Desktop, Documents, Downloads, and Pictures when available. Cached results remain visible while the next search runs; a spinner indicates loading.

## Search by name

Type a file or folder name, or part of it. Exact names, including filenames without the extension, rank above prefixes, substrings, path matches, and fuzzy matches. Recent usage breaks ties; it does not override a better name match. Multiple words can narrow a filename by its parent folder, for example `website readme`.

Fuzzy matching accepts letters in order with gaps: `dcm` matches `Documents`, and `rdme` matches `README.md`. Folder abbreviations can be as short as two letters, such as `dc` for `Documents`; files need at least three letters for this kind of match.

Each result shows its name and parent folder. The list displays up to 50 matches. If a query is too broad, add words or narrow the search folders. Background updates preserve your selected file when it still matches.

Keyword search uses the macOS Spotlight index. Winlane does not create its own content index. Files in folders excluded from Spotlight, or files that have not been indexed yet, might not appear. This version searches names and paths, not document contents.

## Filter by type or expression

Use the type selector below the search field to show **All types**, **Folders**, **Files**, **Documents**, **Images**, **Audio**, **Video**, or **Archives**. File categories use common extensions, ignoring case. Choosing a type with an empty query searches the indexed items of that type. These controls keep their state while Winlane is running.

Enable **`.*`** to match file and folder names with a regular expression. Enter the expression directly, without surrounding slashes. For example:

| Expression | Matches |
| --- | --- |
| `^report.*\.pdf$` | PDF names starting with “report” |
| `\.(png\|jpe?g)$` | PNG, JPG, and JPEG names |
| `^report-\d{4}\.txt$` | Names such as `report-2026.txt` |
| `^Doc.*` | Names starting with “Doc”, including folders |

Expressions match names, not the whole path or file contents, and can be combined with a type filter. Matching ignores case by default; use `(?-i)` for case-sensitive matching. Character classes, groups, alternatives, repetition, and anchors are supported. Look-around and backreferences are not supported. Invalid or overly large expressions show an inline error and can be edited without leaving the panel.

For a specific folder, enter a path followed by an expression, such as `~/Downloads/.*\.pdf$`. Regex compilation and disk/index searches run in the background. A changed expression shows a loading state until matching results arrive; refreshing the same expression keeps its cached results. Broad searches have a processing limit; if the panel reports incomplete results, use a more specific expression or an explicit folder.

## Browse a path

Type **`~/Downloads/`** or an absolute path such as **`/Volumes/Archive/`** to list a folder. Add part of a filename after the final slash to filter its children. A trailing slash selects directory browsing rather than searching for that directory's name.

Press **Tab** to complete the selected result into the search field. A folder gains a trailing slash and its children load in the background; keep typing to narrow them. For example, type `~/dc`, select `Documents`, and press Tab to continue from `~/Documents/`. The cursor stays at the end, so you can repeat this for each level. A file completes its path without opening it; press Enter to open. Use the arrow keys to choose a different result before completing.

For a selected folder, **Enter** browses into it just like Tab. Use **Control + Enter** to open it in Finder. **Control + Backspace** removes the last path component: `~/Downloads/` becomes `~/`, and `~/Downloads/report` becomes `~/Downloads/`. It stops at `~/` or `/` and keeps the file search open.

Completing a result switches off regex mode so the completed path is treated literally.

This reads the directory directly and works without Spotlight. An explicit path can browse outside your configured search scope and exclusions. Hidden entries remain hidden unless the typed filename begins with a dot, such as `~/.config/` or `~/.`.

## Actions

| Key | Action |
| --- | --- |
| Control + T | Show actions for the selected item |
| Tab | Complete the selected path; browse into a selected folder |
| Enter | Browse into the selected folder, or open the selected file with its default app |
| Control + Enter | Open the selected item externally; folders open in Finder |
| Control + Backspace | Remove the last path component and return to its parent |
| Command + Enter | Reveal the selected item in Finder |
| Command + Y | Toggle Quick Look inside the panel |
| Command + C | Copy the file for pasting in Finder |
| Command + Shift + C | Copy its full path |
| Command + R | Refresh the search |
| Esc | Close the panel |

Space remains available in filenames and search terms. Backspace only deletes text, including when the input is empty. The back button returns to the main search. Input follows the same Winlane input-source rule as other command lists.

The **Actions ⌃T** button opens the same menu. It shows the available actions and their shortcuts, including opening, browsing a folder, revealing in Finder, previewing, copying the file, and copying its full path. Escape dismisses the menu while keeping the file search open. Actions apply to the item selected when the menu opened, even if the results refresh in the background.

## Search folders and privacy

In **Settings → Files**, choose the included and excluded folders. Separate multiple paths with semicolons. `~` means your home folder. By default, keyword search covers your home folder, excludes `~/Library`, and hides dotfiles, application bundles, and common build/dependency folders such as `target` and `node_modules`.

Recently opened paths and display metadata are stored locally in `~/Library/Application Support/Winlane/Files/recent.json`. File contents and search queries are not saved. Use **Clear Recents** in Settings → Files to remove that list. Files you open elsewhere do not enter Winlane's recent list.
