# Developing Winlane

Winlane uses Rust for window discovery, search, aliases, and shortcut routing, with native AppKit controls for its interface. Build and run it on macOS with a Rust toolchain and Xcode Command Line Tools installed.

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

Quit the installed app before replacing it at `~/Applications/Winlane.app`, then open that copy. Avoid running the installed and build-directory copies together. Directly running `cargo run` skips app-bundle signing and may result in a different permission identity.

The product is named Winlane, but its bundle identifier remains `app.windowlane.desktop` and the default certificate remains `Windowlane Development` to preserve existing authorization and preferences. By default, the build targets the Rust toolchain's host architecture. Use `--target aarch64-apple-darwin` or `--target x86_64-apple-darwin` to build another architecture after installing that Rust target. `--output-dir DIR` selects a separate bundle directory; it does not change the installation path.

For a disposable build only, `./scripts/build-app.sh --adhoc` skips the certificate requirement. Do not use it to replace a daily development installation whose permissions you want to retain.

See [Releasing Winlane](releasing.md) for DMG/ZIP packaging, CI artifacts, and Developer ID signing. None of these scripts install or launch the app.

## Run checks

```sh
RUSTC_WRAPPER= cargo test --locked
RUSTC_WRAPPER= cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all -- --check
```

The tests cover search ranking, alias stability and uniqueness, shortcut routing, preference migration, language selection, application discovery, and display placement. The native test executable creates hidden AppKit windows to check controls, bilingual layouts, and shared state across displays. It does not register global shortcuts or write preferences.

Real foreground activation, input methods, full-screen Spaces, and physical monitor changes still need interactive testing. The macOS 14 deployment target does not imply that every supported OS version or Intel hardware has been tested; current hands-on validation is on Apple Silicon.

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

The benchmark uses 24 simulated windows and local app icons, with 120 selection moves and 50 searches. Set `WINLANE_BENCH_WINDOWS` to change the list size. It reports initialization and rendering timings, median/P95 latency, RSS, and physical footprint. These measurements exclude real window enumeration, on-screen compositing, and target-app activation. Compare runs with the same display setup, list size, and build profile.

## Platform behavior

Window discovery combines Accessibility results with the system window inventory. An optional private Accessibility lookup recovers standard windows that some apps omit from their published lists, including windows on other Spaces. That recovery path may stop working if macOS changes the underlying interface. Apps with incomplete Accessibility support can still omit windows or reject focus and restore requests.

Switching across Spaces depends on macOS Mission Control settings and the target app. Winlane does not move windows between Spaces. Its recent-window order includes window selections, app shortcuts, and the foreground window captured before opening the picker or launching an app. With recent sorting, the current window appears first and forward switching initially selects the previous window. Other windows of the same app keep their individual positions. Winlane does not continuously monitor switches made through the Dock, mouse, or another tool.

Installed-app search reads Applications directories and the Spotlight index. Apps outside those directories may be missing until indexed. Helpers, internal components, Trash contents, and mounted installer images are excluded.

The catalog is loaded when searching with a nonempty query outside the current-app-only scope, then cached for ten minutes. Subsequent searches use the cached results while an expired catalog updates in the background. Press Command-R with a search query to refresh it immediately, for example after installing an app.

Other keyboard tools can intercept a shortcut before Winlane receives it. If one binding fails while others work, check for remapping rules or competing global shortcuts. Changing Winlane's own bindings does not change those external rules.

## Local state

Preferences use `NSUserDefaults` in the `app.windowlane.desktop` domain. `WindowlanePreferencesV1` stores configuration; `WinlaneAliasesV2` stores app identities, window IDs, and alias assignments. Older alias data is read during migration and preserved. Restoring default settings does not clear aliases or alter the system login item.

Additional window aliases follow window IDs. They survive a Winlane restart while the target windows remain open, but closing and reopening an app creates new IDs and may reassign those aliases. Window titles, search terms, and selection history are not persisted.

The interface reuses rows and shares small app icons between panels. Background results and shortcut events wake the main run loop; a slower timer only checks permissions and shortcut-listener health. Closing the picker stops its redraw work.
