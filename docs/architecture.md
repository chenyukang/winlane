# Code structure

Winlane separates shared Rust logic from macOS integration. The binary starts the native app; the library contains the data models, search rules, configuration, and feature logic.

```text
src/
├── main.rs                 # Binary entry point
├── lib.rs                  # Shared library entry point
├── core/                   # Search, aliases, shortcuts, configuration, localization
├── features/
│   ├── clipboard/          # History model, image assets, persistence
│   ├── projects.rs         # Recent-project discovery and matching
│   ├── quicklinks.rs       # URL templates, validation, imports
│   ├── open_url/           # URL opening, Google search, Chrome history snapshots
│   └── snippets.rs         # Snippet templates and placeholders
└── macos/
    ├── app/                # Application state and feature coordination
    ├── platform/           # macOS APIs, external applications, native persistence
    └── ui/                 # AppKit controls, editors, settings, materials
```

## Responsibilities

`core/` owns shared window/search types, ranking, aliases, shortcut routing, configuration validation, and language selection. `features/` owns the models and operations for individual features. Neither imports AppKit or the native application modules. Configuration refers to feature settings without depending on their editors.

`macos/platform/` wraps Accessibility, window inventory, input sources, event taps, clipboard capture, application launch, system commands, preferences, and Sparkle. It does not depend on `app/` or `ui/`. Application discovery and launch share `applications.rs`; snippet and Quicklink date rendering use `template_context.rs`.

`macos/ui/` owns native controls and editors. Shared controls live in `controls.rs`, shortcut controls in `shortcut.rs`, and glass/frosted materials in `material.rs`. The settings module separates window construction (`layout.rs`), sidebar drawing (`navigation.rs`), and reading or updating controls (`mod.rs`). Preferences storage belongs to the platform layer.

`macos/app/` connects these components. `AppState` remains owned by one main-thread delegate. The delegate's methods are split by responsibility, with visibility limited to the app module; they do not introduce additional state owners or workers.

| Task | Start here |
| --- | --- |
| AppKit callbacks and startup | `macos/app/delegate.rs` |
| Search/switch lifecycle and configured delay | `macos/app/session.rs` |
| Input source, startup buffering, IME composition | `macos/app/input.rs` |
| Global shortcut registration and dispatch | `macos/app/shortcuts.rs` |
| Asynchronous window discovery and icon caches | `macos/app/catalog.rs` |
| Recent-window tracking and exit persistence | `macos/app/recency.rs` |
| Combining and selecting search results | `macos/app/search.rs` |
| Panel construction and display placement | `macos/app/panel.rs` |
| Result rendering and native row drawing | `macos/app/render.rs`, `macos/app/views.rs` |
| Settings changes and language rebuilds | `macos/app/preferences.rs` |
| Feature-specific actions | `macos/app/clipboard.rs`, `projects.rs`, `quicklinks.rs`, `snippets.rs`, `commands.rs` |

## Tests

The integration tests in `tests/*.rs` exercise the shared library. `tests/native_panels.rs` is a small main-thread runner that imports the production macOS module tree. Native test helpers live in `tests/native/` and are attached to their owning modules only under `cfg(test)`. Tests do not maintain a second list of production modules or include copies of production source into substitute modules.

`tests/native/app/` groups panel layout, input startup, recency, asynchronous refresh, and feature interaction checks. Optional diagnostic and benchmark entry points are in `diagnostics.rs`; their environment variables and behavior remain documented in the [development guide](development.md#native-diagnostics).

After changing a module, run:

```sh
cargo fmt --all -- --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

Native tests create hidden windows and use isolated test data. Real desktop activation, input methods, and full-screen Spaces still require separate interactive verification. See [development](development.md) for build and installation instructions, and [commands](commands.md#add-a-command) for the command extension point.
