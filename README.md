# Winlane

A native macOS window switcher, built with Rust and AppKit.

Hold a shortcut to move between windows, or type to find the one you need. Winlane runs in the menu bar and stays out of the way until you call it up.

![Winlane in switch mode, showing window aliases and keyboard navigation](docs/images/window-switcher.png)

## Get started

You need **macOS 14 or later**. Tagged builds are distributed through [GitHub Releases](https://github.com/chenyukang/winlane/releases) as a DMG or ZIP:

- **arm64** for Apple Silicon Macs.
- **x86_64** for Intel Macs.

Open the DMG and drag Winlane to Applications, or extract the ZIP and move `Winlane.app` there. Quit an existing copy before replacing it, and keep the same installation location for updates. Each release includes SHA-256 checksums and its signing status. Until the first release is published, use the source build below.

If a release is marked **ad hoc / not notarized**, macOS may block the first launch. After verifying the download, use **System Settings → Privacy & Security → Open Anyway**. These builds may need Accessibility permission again after an update. Developer ID signed and notarized releases avoid the ad hoc identity changes.

### Build from source

Install a Rust toolchain and Xcode Command Line Tools.

First, [set up a code-signing identity](docs/development.md#development-signing). Keeping the same identity lets development builds retain their Accessibility permission.

```sh
git clone https://github.com/chenyukang/winlane.git
cd winlane
./scripts/build-app.sh
```

The script builds a release version at `dist/Winlane.app`. Quit any running copy, copy the app to `~/Applications/Winlane.app`, and open it. Use this same installation location for subsequent builds.

### Allow window access

Open **System Settings → Privacy & Security → Accessibility**, add the installed Winlane app, and enable access. On macOS 27, this pane is called **Device Control and Data Access**. Open Winlane's panel again after granting permission.

Once authorized, Winlane starts quietly in the menu bar. Its menu provides access to the window picker, settings, and Quit.

## Everyday use

### Switch without stopping to search

Hold **Command**, press **Tab** to open the switcher, then keep pressing Tab to move through the list. Release Command to switch to the selected window. Add Shift to move backward, or press Esc to cancel.

Each row represents an independent window. Separate editor projects appear separately; browser tabs and background helpers do not become extra entries. On multiple displays, the same picker appears on every screen, with a shared selection.

The letters beside each window are its **alias**. Type those letters while holding Command, then release Command to jump directly to that window. Aliases are assigned automatically from English app names, stay unique across windows, and are remembered when Winlane restarts. Additional window aliases may change when the target app closes and creates new windows.

### Search when you know what you want

Press **Control + Option + Space** to open search. Type an app name, a window title, an alias, or a few keywords. Use the arrow keys to select a result and Enter to open it.

An empty search shows existing windows. Once you type, matching installed apps also appear with a **↗ Launch app** label, so you can open an app that is not running yet. Apps that already have windows are not repeated as launch results.

Press **Space** in the switcher to start searching. In an empty search field, Space returns to switching; with text already entered, it remains a normal space. Input-method composition keeps its usual keyboard behavior.

### Give frequent apps a permanent shortcut

Open **Settings → Shortcuts → App Shortcuts**, choose an app, and assign a combination such as **Command + 1** for your browser or **Command + 2** for chat. Save to enable it globally.

These shortcuts work without opening the picker and launch the app if needed. They target an application; use a window's alias when you need a particular project or document. A global binding overrides the same combination in other apps—for example, a browser's numbered-tab shortcut.

## Make it yours

Open Settings from the menu bar or press **Command + ,** while using Winlane.

| Settings tab | What you can change |
| --- | --- |
| **Shortcuts** | Separate shortcuts for search and switching, plus fixed app shortcuts. |
| **Appearance & Language** | System, light, or dark appearance; system language, Chinese, or English. |
| **Window List** | Sorting, minimized windows, and apps to exclude. Recent sorting reflects selections made through Winlane. |
| **Startup** | Launch at login and access to macOS login-item settings. |

Click **Save Settings** to apply changes. Language changes update the interface immediately, without a restart. By default, Winlane uses the first supported language in your macOS preferences, falling back to English. The launch-at-login toggle takes effect immediately, independently of Save.

The search panel's **Window Actions** menu also lets you minimize or restore a window, hide its app, and copy its title. **Command + R** refreshes the list if something looks out of date.

## Privacy and permissions

Winlane uses Accessibility access to read and control windows, and a keyboard event filter to handle its shortcuts. It does not log keystrokes, capture window screenshots, or send window data to a server.

Settings and aliases are stored locally. Window titles, search terms, and selection history stay in memory and are discarded when Winlane quits. Copying a window title explicitly places it on the clipboard.

## Development

```sh
RUSTC_WRAPPER= cargo test --locked
RUSTC_WRAPPER= cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all -- --check
```

CI runs formatting, Clippy, tests, and packaging checks on Apple Silicon and Intel macOS runners. See the [development guide](docs/development.md) for local signing and diagnostics, and the [release guide](docs/releasing.md) for binary distribution and Apple notarization.
