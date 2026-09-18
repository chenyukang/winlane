# Quicklinks

Quicklinks open saved websites, app links, files, and folders. Add or edit them in **Settings → Quicklinks**. Valid changes save automatically in the existing settings window; an invalid edit leaves the last saved version intact.

Type a link's name or part of its address in ordinary search, then select it and press Enter. Matching windows come first, so window aliases keep their priority. Search for **quicklink** (or **快捷链接**) and select that command to browse only links. An empty query in this view lists all saved links. Esc, the back button, or Backspace in an empty query returns to ordinary search. Quicklinks do not appear in switch mode.

## Link templates

| Template | Behavior |
| --- | --- |
| `https://example.com/docs` | Open a website. A bare hostname such as `example.com/docs` gets `https://`. |
| `~/Downloads` | Open a local folder. Absolute paths also work. |
| `example-app://open` | Open an app's registered URL scheme. |
| `https://example.com/search?q={Query}` | Ask for a Query value before opening. |
| `https://example.com/search?q={argument name="Query" default="rust"}` | Use an editable default value. |
| `https://example.com/search?q={clipboard}` | Insert current clipboard text. |
| `{clipboard | raw}` | Open the copied URL directly. |
| `https://example.com/archive/{date format="yyyy-MM-dd"}` | Insert today's date. |

URL placeholders are percent-encoded, including spaces, Chinese text, `/`, `&`, and `#`. Static URL syntax remains unchanged. Add `| raw` only when the value should supply URL syntax itself. Local path placeholders remain ordinary path text.

`{Query}` is shorthand for `{argument name="Query"}`. Repeated named fields share one value; separate unnamed `{argument}` placeholders create separate inputs. Up to eight input fields are supported. Select a Quicklink and press Enter or Tab to fill its parameters directly in the search panel. Tab and Shift+Tab move between fields; Enter opens the completed destination once required fields are filled. The result row previews the destination. Esc or the back button returns to the previous search, preserving its query and selection. Backspace in an empty first field also returns to search. No separate input window opens.

Leave **Open with** empty to use the system's default app. To choose another app, enter its name, bundle identifier, or application path. Opening a link uses macOS URL handling; link text is never interpreted as a shell command.

## Import from Raycast

1. Run **Export Quicklinks** in Raycast and save the JSON file.
2. Choose **Settings → Quicklinks → Import Raycast JSON…** in Winlane.
3. Select the export file. Winlane reports how many links were added or skipped.

The importer accepts Raycast's [Quicklinks JSON format](https://manual.raycast.com/quicklinks): an array with `name`, `link`, and optional `openWith`. Icons are represented by Winlane's link icon. Entries with identical names, links, and target apps are skipped, making repeated imports safe. Existing entries and unfinished editor drafts are preserved. The entire import is validated before saving; an invalid entry leaves the library unchanged.

This supports `{Query}`, named arguments, clipboard text, date/time formats, and `raw` placeholders. Other Raycast modifiers are not imported silently: unsupported syntax produces an error. Import does not synchronize later Raycast changes, import hotkeys, or require access to Raycast's database.

Up to 200 links are stored in the `quicklinks` field of Winlane's preferences, alongside snippets and other settings. Links stay on this Mac and survive app updates. No link is opened or fetched during import. Link templates are limited to 16 KB, and import files to 4 MB.
