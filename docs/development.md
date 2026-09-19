# Developing Winlane

Winlane uses Rust for window discovery, search, aliases, and shortcut routing, with native AppKit controls for its interface. Build and run it on macOS with a Rust toolchain and Xcode Command Line Tools installed.

See [code structure](architecture.md) for module responsibilities and the files to change for each feature.

## Development signing

Use the same signing identity, bundle identifier, and installation path across builds. An ad hoc signature changes with the executable and can invalidate the app's Accessibility authorization. A stable certificate lets debug and release builds share an identity.

For local development, create a self-signed code-signing certificate once:

1. Open **Keychain Access → Certificate Assistant → Create a Certificate…**.
2. Use **Windowlane Development** as the name, **Self Signed Root** as the identity type, and **Code Signing** as the certificate type.
3. Select **Let me override defaults**, choose a suitable validity period, and save the certificate in your **login** keychain. Broad “Always Trust” settings are not required.
4. Run the build script. If macOS asks to let `codesign` use the private key, verify the requesting program and certificate. Choosing **Always Allow** avoids repeated private-key prompts during builds.

Keep the certificate and private key in the keychain, outside the repository. The build script looks for one identity named `Windowlane Development` and stops if the identity is missing or ambiguous. It never silently falls back to ad hoc signing.

You can use another code-signing identity by passing its exact name or SHA-1 fingerprint:

```sh
security find-identity -p codesigning
WINLANE_SIGNING_IDENTITY='Your signing identity' ./scripts/build-app.sh
```

The legacy `WINDOWLANE_SIGNING_IDENTITY` environment variable also works; `WINLANE_SIGNING_IDENTITY` takes precedence.

When moving from an ad hoc build to a certificate-signed build, remove the old permission entry and authorize the installed app once. Subsequent builds should retain access while the identity and location stay unchanged. Local self-signed builds are not a substitute for Developer ID signing and notarization for public distribution.

## Build and install

```sh
# Optimized release build—the default for everyday use.
./scripts/build-app.sh

# Debug build, using the same signing identity.
./scripts/build-app.sh --debug
```

Both commands produce `dist/Winlane.app`. The script builds with locked dependencies, bundles the app icon, signs the app, and verifies the signature. It does not install or launch the result.

These development bundles do not load Sparkle or check for updates. Add `--with-updater` to build the same updater-enabled bundle used for releases. This downloads a SHA-256-pinned Sparkle distribution to `target/sparkle/`, embeds the framework, and signs its helpers before signing the app. The manual update menu and update settings are disabled when Sparkle is not bundled.

Quit the installed app before replacing it at `~/Applications/Winlane.app`, then open that copy. Avoid running the installed and build-directory copies together. Directly running `cargo run` skips app-bundle signing and may result in a different permission identity.

The product is named Winlane, but its bundle identifier remains `app.windowlane.desktop` and the default certificate remains `Windowlane Development` to preserve existing authorization and preferences. By default, the build targets the Rust toolchain's host architecture. Use `--target aarch64-apple-darwin` or `--target x86_64-apple-darwin` to build another architecture after installing that Rust target. `--output-dir DIR` selects a separate bundle directory; it does not change the installation path.

For a disposable build only, `./scripts/build-app.sh --adhoc` skips the certificate requirement. Do not use it to replace a daily development installation whose permissions you want to retain.

See [Releasing Winlane](releasing.md) for DMG/ZIP packaging, CI artifacts, and Developer ID signing. The build and packaging scripts do not install or launch the app.

## Run checks

```sh
RUSTC_WRAPPER= cargo test --locked
RUSTC_WRAPPER= cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all -- --check
```

The tests cover search ranking, alias stability and uniqueness, shortcut routing, preference migration, language selection, application discovery, and display placement. The native test executable creates hidden AppKit windows to check controls, bilingual layouts, and shared state across displays. It does not register global shortcuts or write preferences.

Real foreground activation, input methods, full-screen Spaces, and physical monitor changes still need interactive testing. The macOS 14 deployment target does not imply that every supported OS version or Intel hardware has been tested; current hands-on validation is on Apple Silicon.

To test Sparkle separately:

```sh
./scripts/build-app.sh --with-updater --output-dir dist/updater
./scripts/test-updater.sh dist/updater/Winlane.app
python3 scripts/test-sparkle-update.py dist/updater/Winlane.app
```

The first probe exercises the Rust bridge, persisted automatic-check preference, and disabled/missing framework paths in a disposable bundle. The end-to-end test uses temporary app identities, ephemeral signing keys, and a loopback HTTP server. It downloads, validates, installs, and relaunches a fixture app, then verifies that altered archives and feeds are rejected. Neither test registers Winlane's shortcuts, changes Accessibility authorization, or replaces the installed app. Test preferences are removed afterward.

### Native diagnostics

The `native_panels` test executable supports several optional checks. Run them separately; each environment variable selects a diagnostic mode.

```sh
# Discover installed apps without launching them.
RUSTC_WRAPPER= WINLANE_APP_SCAN=1 cargo test --locked --test native_panels

# Launch a temporary background fixture, reopen it, then remove it.
RUSTC_WRAPPER= WINLANE_LAUNCH_SMOKE=1 cargo test --locked --test native_panels

# Read an app's real windows without activating them.
RUSTC_WRAPPER= WINLANE_SCAN_BUNDLE=com.microsoft.VSCode WINLANE_EXPECT_WINDOWS=3 \
  cargo test --release --locked --test native_panels

# Measure hidden-panel rendering and memory use.
RUSTC_WRAPPER= WINLANE_BENCHMARK=1 cargo test --release --locked --test native_panels
```

The window scan requires the test executable to already have Accessibility access. It performs twelve scans; adjust `WINLANE_EXPECT_WINDOWS` to the number of independent windows open, or omit it to report counts without asserting a minimum. Adding `WINLANE_SCAN_TRANSITIONS=1` checks recovery when a simulated published window list changes, using at least three real windows. It does not switch Spaces, close windows, or activate them.

The launch check exercises search-result launching from a stopped state, process reuse, and rejection of a mismatched bundle identifier. Its fixture stays in the background, so it does not verify foreground activation of everyday apps.

The benchmark uses 24 simulated windows and local app icons, with 120 selection moves, 50 searches, and 30 cached search-panel preparations. Set `WINLANE_BENCH_WINDOWS` to change the list size. It reports initialization, first-list rendering, deferred icon loading, median/P95 latency, RSS, and physical footprint. Panel preparation includes cached filtering and layout on all displays. These measurements exclude real window enumeration, foreground capture, input-source activation, on-screen compositing, and target-app activation. Compare runs with the same display setup, list size, and build profile.

Opening a panel renders cached results once. Application metadata, focus notifications, and search-mode focus queries run in workers; missing icons initially use a shared placeholder and are filled in on later run-loop turns. While hidden, the panel prepares a bounded row pool one row at a time. Search presents without an intentional delay. Switching retains the configured delay, cancellation behavior, and bounded exact foreground capture so a quick release can immediately commit the correct window. Background focus results cannot overwrite newer visits or reorder a held switch gesture, and search refreshes preserve the selected result.

## Platform behavior

Optional window-order diagnostics can be enabled with `defaults write app.windowlane.desktop WinlaneTraceRecency -bool true`, followed by a normal quit and restart. Disable with the same command using `false` and restart. The default is off. Logs are written to `~/Library/Logs/Winlane/recency.log` with one rotated `recency.previous.log`, about 2 MiB per file. They append across restarts and do not expire after a fixed duration. Entries contain timestamps, process/window IDs, app bundle IDs, focus-query errors and the recent/displayed order, never window titles or search text. A bounded background queue keeps file I/O off the shortcut path; a `dropped` count marks events lost if the writer falls behind. This diagnostic switch is separate from saving recent-window order on normal exit.

Search scopes, inline Quicklink arguments, settings editors, and snippet argument forms share one input-source preference, which defaults to English. Restoring the last source used in Winlane requires explicitly choosing Last Used. Preparing a command's search before presenting its panel preserves the selected startup source; it is cancelled only after a focused search loses focus or the session ends. Entering a command from an already active search resolves the preference again without replacing the original application's input-source restore record. Result refreshes and ordinary keystrokes do not reset the preference.

The shared platform resolver chooses the source, startup language hint, and keyboard-layout insertion behavior. Settings and snippet forms apply it through the common native window class when a window or a new editable field gains focus; hidden windows never select an input source. Existing composition and refocusing the same field are preserved. Leaving a window restores its original source only if another application has not already changed it.

Search resolves an enabled input source from the saved English, Chinese, or Last Used policy; Current and unavailable sources require no switch. It selects that source before focusing its field editor, then aligns the editor's input context, skipping either request when it already has the target source. Initial key-down events wait until both report that source, preserving the order of text, editing keys, and Return. Escape and focus loss discard pending events. The short-lived readiness timer stops immediately on completion; it gives up after 250 ms if macOS cannot activate the source, so input cannot remain blocked. Manual input-source changes after startup remain supported.

The panel's initial responder is a non-text content view. Reopening also resets the remembered responder to that view. This keeps AppKit from activating the previous IME before `windowDidBecomeKey`: the panel takes keyboard focus first, selects the configured source, then explicitly focuses the search editor. The cell also receives temporary `allowedInputSourceLocales` as a language hint; that hint alone does not prevent every third-party IME from activating. English and Chinese use their configured language; Last Used takes the saved source's primary language when available. The constraint is removed from the cell and live editor when startup ends or is cancelled. Queued keys are released only after presentation returns, the editor's context is active, and the unrestricted source matches the target, or the bounded timeout expires. This adds no fixed presentation delay and does not change the global input source while preparing hidden panels. Native checks cover fresh and reused non-text responders, all four policies, and preservation of Chinese composition.

Result refreshes preserve marked text in the active search field. During IME composition, the field can contain pinyin that is not yet in the shared query; copying that query back would erase the first letters. Only starting a new search discards the previous editing session. Native tests cover composition across repeated refreshes, committing Chinese text, and reopening the picker without stale composition.

An IME can continue composing briefly even after both source APIs report ABC. When the configured source is a keyboard layout and that exact source is still selected, search re-translates ordinary printable ASCII keys with `NSEvent.characters(byApplyingModifiers:)` and inserts them through the native editor directly. This avoids forwarding those keys to the previous IME. It uses the actual layout and event modifiers, including Shift and Caps Lock. Shortcuts, editing commands, Option/dead-key input, non-ASCII text, and existing composition keep AppKit's normal dispatch. Chinese and Current policies do not enable this path, and selecting a different input source disables it immediately. Native checks cover immediate committed text, stale event characters, selection replacement, shared query updates, and preserving composition.

Window discovery combines Accessibility results with the system window inventory. An optional private Accessibility lookup recovers standard windows that some apps omit from their published lists, including windows on other Spaces. That recovery path may stop working if macOS changes the underlying interface. Apps with incomplete Accessibility support can still omit windows or reject focus and restore requests.

The foreground application's Accessibility observer also listens for destroyed windows and removes known closed windows from cached and deferred lists. Focus changes, application activation/termination, and opening a picker trigger a coalesced background WindowServer check as a fallback for missing notifications. This check uses all windows, including offscreen and minimized windows; a failed check or an unknown server ID does not remove an entry. During a held switch gesture, removal preserves the remaining order and selected window; closing the selected window advances to the next survivor. A late Accessibility scan cannot restore windows already removed during that scan. No periodic window scan or synchronous WindowServer query is added to presentation.

Switching across Spaces depends on macOS Mission Control settings and the target app. Winlane does not move windows between Spaces. Its recent-window order includes window selections, app shortcuts, and the foreground window captured when opening the picker or launching an app. With recent sorting, the current window appears first and forward switching initially selects the previous window. Other windows of the same app keep their individual positions. Application activation notifications track Dock and external switches; an Accessibility observer tracks focused-window changes within the foreground app. Only that app is observed, and no periodic window scan is added. Search opens with the latest cached focus while a background query checks it. If that query cannot read the focused window, it records the app's cached window instead; stale results cannot override newer visits or application switches. Switching captures the foreground window before selecting, including for applications that omit focus notifications.

Installed-app search reads Applications directories and the Spotlight index. Apps outside those directories may be missing until indexed. Helpers, internal components, Trash contents, and mounted installer images are excluded.

The catalog is loaded when searching with a nonempty query outside the current-app-only scope, then cached for ten minutes. Subsequent searches use the cached results while an expired catalog updates in the background. Press Command-R with a search query to refresh it immediately, for example after installing an app.

Foreground capture on the shortcut path uses one focus query with a 20 ms messaging timeout and no retries. Background focus queries use a 120 ms timeout, with at most one 800 ms retry for a messaging failure or a temporarily missing focus value. Unsupported attributes, invalid elements, and permission errors are not retried. These are Accessibility messaging timeouts, not delays added before presenting the picker. A superseded query is detached; a result received after a newer visit or application switch cannot change recency or the current selection.

Other keyboard tools can intercept a shortcut before Winlane receives it. If one binding fails while others work, check for remapping rules or competing global shortcuts. Changing Winlane's own bindings does not change those external rules.

## Local state

Preferences use `NSUserDefaults` in the `app.windowlane.desktop` domain. `WindowlanePreferencesV1` stores configuration; `WinlaneAliasesV2` stores app identities, window IDs, and alias assignments. Older alias data is read during migration and preserved. Restoring default settings does not clear aliases or alter the system login item.

Additional window aliases follow window IDs. They survive a Winlane restart while the target windows remain open, but closing and reopening an app creates new IDs and may reassign those aliases.

`WinlaneRecentWindowsV1` stores up to 128 recent window IDs in most-recent-first order. Winlane saves this history only on normal termination and restores it before tracking the current foreground window on startup. Window switching does not write history to preferences. A crash or forced termination leaves the last normally saved order unchanged. Closed windows are ignored when sorting, and temporarily missing windows retain their history. This restores the order of windows that remain open across a Winlane restart; restarting another app creates new window IDs. Window titles and search terms are not persisted.

The interface reuses rows and shares small app icons between panels. Background results and shortcut events wake the main run loop; a slower timer only checks permissions and shortcut-listener health. Closing the picker stops its redraw work.
