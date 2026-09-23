mod auto_appclose;
mod bluetooth;
mod catalog;
mod clipboard;
mod commands;
mod delegate;
mod emoji;
mod files;
mod input;
mod input_indicator;
mod input_rules;
mod keep_awake;
mod launch;
mod layout;
mod meeting;
mod menus;
mod open_url;
mod panel;
mod preferences;
mod projects;
mod quicklinks;
mod recency;
mod render;
mod scrolling;
mod search;
mod session;
mod settings_focus;
mod shortcuts;
mod snippets;
mod views;
mod window_liveness;
mod windows;

use crate::macos::ui::material::{PanelBackdrop, tint};
use catalog::demo_windows;
use delegate::Delegate;
#[cfg(test)]
use input::insert_keyboard_layout_text;
use layout::{alias_badge_size, label, panel_height, rect, row_height, set_label};
use settings_focus::PendingSettingsFocus;
use views::{ListView, PanelContentView, RowAppearance, SearchPanel, WindowRowButton};

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use winlane::{tr, trf};

use crate::macos::platform::main_wake::MainWake;
use objc2::rc::{Retained, Weak};
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{AnyThread, DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::*;
use objc2_foundation::{
    MainThreadMarker, NSArray, NSBundle, NSData, NSNotification, NSNotificationCenter, NSNumber,
    NSObject, NSObjectProtocol, NSPoint, NSRect, NSRunLoop, NSRunLoopCommonModes, NSSize, NSString,
    NSTimer, NSURL, NSUserDefaults, ns_string,
};
use std::time::{Duration, Instant};
use winlane::core::aliases::{AliasInput, AliasMatch, Aliases, AppIdentity};
use winlane::core::app_catalog::{InstalledApp, matching_apps};
use winlane::core::commands::{CommandId, matching_commands};
use winlane::core::config::{ApplicationTarget, Config, DisplayDensity, visible_matches};
use winlane::core::discovery::FocusRead;
use winlane::core::displays::{Display, Rect, placements};
use winlane::core::search::WindowInfo;
use winlane::core::shortcuts::{
    Action, ActionKind, PanelCommand, PanelMode, SPACE, SwitchSelection, panel_command,
    space_changes_mode,
};

use crate::macos::platform::accessibility;
use crate::macos::platform::input_source::{self, Source};
use crate::macos::platform::installed_apps;
use crate::macos::platform::shortcut_tap::ShortcutTap;
use crate::macos::ui::app_shortcuts::AppShortcutsWindow;
use crate::macos::ui::settings::SettingsWindow;
#[cfg(test)]
use winlane::core::input_method::InputMethod;
use winlane::core::input_method::{InputGate, InputSession};

const WIDTH: f64 = 700.0;
const HEIGHT: f64 = 590.0;
const MODE_LABEL_SPACING: f64 = 28.0;
const LIST_TOP: f64 = 526.0;
const LIST_BOTTOM: f64 = 36.0;
// Slimmer bottom inset used when no footer row (hints, error, or permission prompt) shows.
const LIST_BOTTOM_BARE: f64 = 26.0;
const LIST_WIDTH: f64 = WIDTH - 20.0;
const APP_CATALOG_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Copy, PartialEq, Eq)]
enum SearchScope {
    Files,
    KeepAwake,
    Bluetooth,
    Projects,
    OpenUrl,
    Meeting,
    Quicklinks,
    Snippets,
    Emoji,
    Clipboard,
}

struct PanelUi {
    file_controls: files::Controls,
    file_preview: RefCell<Option<Retained<NSView>>>,
    display_id: u32,
    shortcut_label: Retained<NSTextField>,
    project_progress: Retained<NSProgressIndicator>,
    panel: Retained<SearchPanel>,
    backdrop: PanelBackdrop,
    input: Retained<NSSearchField>,
    scroll: Retained<NSScrollView>,
    list: Retained<ListView>,
    footer: Retained<NSTextField>,
    help: Retained<NSButton>,
    demo_button: Retained<NSButton>,
    refresh_button: Retained<NSButton>,
    settings_button: Retained<NSButton>,
    history_permissions_button: Retained<NSButton>,
    scope_back: Retained<NSButton>,
    clipboard_actions: Retained<NSPopUpButton>,
    quicklink_bar: RefCell<crate::macos::ui::quicklinks::input::Bar>,
    mode_label: Retained<NSTextField>,
    rows: RefCell<Vec<RowUi>>,
    empty_labels: RefCell<Vec<Retained<NSTextField>>>,
}

struct RowUi {
    button: Retained<WindowRowButton>,
    title: Retained<NSTextField>,
    app: Retained<NSTextField>,
    alias: Retained<NSTextField>,
    icon: Retained<NSImageView>,
    content: Option<RowContent>,
    selected: Option<bool>,
    attached: bool,
}

#[derive(Clone, PartialEq, Eq)]
enum RowContent {
    File(winlane::features::files::Entry),
    KeepAwake(winlane::features::keep_awake::Choice),
    Bluetooth(winlane::features::bluetooth::Device, Option<bool>),
    Project(winlane::features::projects::Project),
    OpenUrl(winlane::features::open_url::Page),
    Meeting(winlane::features::meeting::Meeting),
    Command(CommandId),
    Snippet(winlane::features::snippets::Snippet),
    Emoji(winlane::features::emoji::Emoji),
    Quicklink(winlane::features::quicklinks::Quicklink),
    Clipboard(u64, String, String, String),
    Window(WindowInfo, Option<String>),
    Application(ApplicationTarget),
}

enum SelectedResult {
    File(std::path::PathBuf),
    KeepAwake(winlane::features::keep_awake::Choice),
    Bluetooth(String),
    Project(std::path::PathBuf),
    OpenUrl(String),
    Meeting(String),
    Command(CommandId),
    Snippet(String),
    Emoji(&'static str),
    Quicklink(String),
    Clipboard(u64),
    Window(u64),
    Application(String),
}

#[derive(Clone, Copy)]
enum LaunchOrigin {
    Shortcut,
    Search,
}

struct PendingLaunch {
    receiver: Receiver<Result<i32, String>>,
    origin: LaunchOrigin,
}

struct WindowSnapshot {
    windows: Vec<WindowInfo>,
    identities: HashMap<i32, AppIdentity>,
    server_ids: HashMap<u64, u32>,
}

struct PendingFocus {
    receiver: Receiver<Option<u64>>,
    pid: i32,
    revision: u64,
    recency: Vec<u64>,
}

#[derive(Default)]
struct AppState {
    emoji_matches: RefCell<Vec<winlane::features::emoji::Emoji>>,
    auto_appclose: RefCell<auto_appclose::State>,
    files: RefCell<files::State>,
    keep_awake: RefCell<crate::macos::platform::keep_awake::KeepAwake>,
    keep_awake_matches: RefCell<Vec<winlane::features::keep_awake::Choice>>,
    keep_awake_error: RefCell<Option<String>>,
    keep_awake_indicator: RefCell<Option<crate::macos::ui::keep_awake_indicator::Indicator>>,
    panels: RefCell<Vec<Rc<PanelUi>>>,
    query: RefCell<String>,
    search_scope: Cell<Option<SearchScope>>,
    scoped_refresh_timer: RefCell<Option<Retained<NSTimer>>>,
    current_app_only: Cell<bool>,
    keyboard_display: Cell<Option<u32>>,
    changing_displays: Cell<bool>,
    syncing_controls: Cell<bool>,
    preparing_panel: Cell<bool>,
    saving_settings: Cell<bool>,
    input_session: RefCell<InputSession>,
    input_indicator: RefCell<Option<crate::macos::ui::input_indicator::Indicator>>,
    input_target: RefCell<Option<Source>>,
    input_layout_source: RefCell<Option<String>>,
    input_start_locales: RefCell<Option<Retained<NSArray<NSString>>>>,
    input_gate: RefCell<InputGate<Retained<NSEvent>>>,
    input_start_timer: RefCell<Option<Retained<NSTimer>>>,
    input_start_deadline: Cell<Option<Instant>>,
    changing_input_source: Cell<bool>,
    check_panel_focus: Cell<bool>,
    config: RefCell<Config>,
    config_store: OnceCell<Retained<NSUserDefaults>>,
    aliases: RefCell<Aliases>,
    automatic_aliases: RefCell<Aliases>,
    identities: RefCell<HashMap<i32, AppIdentity>>,
    icons: RefCell<HashMap<i32, Option<Retained<NSImage>>>>,
    placeholder_icon: OnceCell<Option<Retained<NSImage>>>,
    cache_warmup_timer: RefCell<Option<Retained<NSTimer>>>,
    #[cfg(test)]
    render_passes: Cell<usize>,
    alias_input: RefCell<AliasInput>,
    alias_error: RefCell<Option<String>>,
    aliases_writable: Cell<bool>,
    settings: RefCell<Option<Rc<SettingsWindow>>>,
    settings_release_pending: Cell<bool>,
    settings_focus_pending: Cell<Option<PendingSettingsFocus>>,
    settings_focus_timer: RefCell<Option<Retained<NSTimer>>>,
    app_input_rules: RefCell<winlane::features::input_rules::Runtime>,
    app_input_pending: Cell<bool>,
    settings_last_tab: Cell<Option<isize>>,
    snippet_editor_draft: RefCell<Option<crate::macos::ui::snippets::SnippetEditorDraft>>,
    quicklink_editor_draft: RefCell<Option<crate::macos::ui::quicklinks::QuicklinkEditorDraft>>,
    updater: OnceCell<crate::macos::platform::updater::Updater>,
    updater_error: RefCell<Option<String>>,
    app_shortcuts: RefCell<Option<Rc<AppShortcutsWindow>>>,
    alias_rules_editor: RefCell<Option<Rc<crate::macos::ui::alias_rules::AliasRulesEditor>>>,
    alias_rules_draft: RefCell<Option<crate::macos::ui::alias_rules::AliasRulesDraft>>,
    launch_receiver: RefCell<Option<PendingLaunch>>,
    installed_apps: RefCell<Vec<InstalledApp>>,
    catalog_receiver: RefCell<Option<Receiver<Vec<InstalledApp>>>>,
    catalog_checked: Cell<Option<Instant>>,
    catalog_watcher: RefCell<Option<crate::macos::platform::catalog_watcher::CatalogWatcher>>,
    catalog_generation: Cell<u64>,
    catalog_scan_generation: Cell<u64>,
    application_icons: RefCell<HashMap<String, Retained<NSImage>>>,
    show_menu_item: RefCell<Option<Retained<NSMenuItem>>>,
    switch_menu_item: RefCell<Option<Retained<NSMenuItem>>>,
    status_item: OnceCell<Retained<NSStatusItem>>,
    timer: OnceCell<Retained<NSTimer>>,
    shortcut_tap: RefCell<Option<ShortcutTap>>,
    scroll_tap: RefCell<Option<crate::macos::platform::scrolling::ScrollTap>>,
    scroll_error: RefCell<Option<String>>,
    shortcut_rx: RefCell<Option<Receiver<Action>>>,
    last_shortcut_check: Cell<Option<Instant>>,
    wake: OnceCell<MainWake>,
    mode: Cell<Option<PanelMode>>,
    session: Cell<u64>,
    switch_selection: RefCell<Option<SwitchSelection>>,
    switch_anchor: Cell<Option<u64>>,
    switch_timer: RefCell<Option<Retained<NSTimer>>>,
    deferred_windows: RefCell<Option<Vec<WindowInfo>>>,
    hotkey_error: RefCell<Option<String>>,
    windows: RefCell<Vec<WindowInfo>>,
    matches: RefCell<Vec<usize>>,
    launch_matches: RefCell<Vec<usize>>,
    command_matches: RefCell<Vec<CommandId>>,
    snippet_matches: RefCell<Vec<winlane::features::snippets::Snippet>>,
    bluetooth_devices: RefCell<Vec<winlane::features::bluetooth::Device>>,
    bluetooth_matches: RefCell<Vec<winlane::features::bluetooth::Device>>,
    bluetooth_error: RefCell<Option<String>>,
    bluetooth_receiver: RefCell<Option<Receiver<winlane::features::bluetooth::Update>>>,
    bluetooth_pending: RefCell<Option<(String, bool)>>,
    bluetooth_permission: RefCell<Option<Retained<AnyObject>>>,
    open_url_matches: RefCell<Vec<winlane::features::open_url::Page>>,
    open_url_history: RefCell<winlane::features::open_url::History>,
    open_url_receiver: RefCell<Option<Receiver<winlane::features::open_url::History>>>,
    meeting_matches: RefCell<Vec<winlane::features::meeting::Meeting>>,
    meeting_results: RefCell<winlane::features::meeting::Meetings>,
    meeting_receiver: RefCell<Option<Receiver<winlane::features::meeting::Meetings>>>,
    meeting_day_offset: Cell<i32>,
    project_matches: RefCell<Vec<winlane::features::projects::Project>>,
    project_cache: RefCell<winlane::features::projects::Cache>,
    project_receiver: RefCell<Option<Receiver<projects::ProjectUpdate>>>,
    project_open: RefCell<Option<crate::macos::platform::project_open::PendingProjectOpen>>,
    quicklink_matches: RefCell<Vec<winlane::features::quicklinks::Quicklink>>,
    quicklink_input: RefCell<Option<crate::macos::ui::quicklinks::input::Input>>,
    quicklink_editor: RefCell<Option<Retained<crate::macos::ui::quicklinks::QuicklinkEditor>>>,
    clipboard_matches: RefCell<Vec<u64>>,
    clipboard: RefCell<Option<crate::macos::platform::clipboard::ClipboardRuntime>>,
    clipboard_timer: RefCell<Option<Retained<NSTimer>>>,
    snippet_editor: RefCell<Option<Retained<crate::macos::ui::snippets::SnippetEditor>>>,
    snippet_arguments: RefCell<Option<Retained<crate::macos::ui::snippets::SnippetArguments>>>,
    snippet_paste_timer: RefCell<Option<Retained<NSTimer>>>,
    selected: Cell<usize>,
    previous_pid: Cell<i32>,
    previous_window: Cell<Option<u64>>,
    receiver: RefCell<Option<Receiver<WindowSnapshot>>>,
    window_server_ids: RefCell<HashMap<u64, u32>>,
    window_check_receiver: RefCell<Option<Receiver<Vec<u64>>>>,
    window_check_again: Cell<bool>,
    closed_windows: RefCell<HashSet<u64>>,
    loading: Cell<bool>,
    demo: Cell<bool>,
    recency: RefCell<Vec<u64>>,
    recency_store: OnceCell<Retained<NSUserDefaults>>,
    focus_observer: RefCell<Option<crate::macos::platform::focus_observer::FocusObserver>>,
    focus_receiver: RefCell<Option<PendingFocus>>,
    focus_pid: Cell<i32>,
    focus_revision: Cell<u64>,
    preferences: RefCell<HashMap<String, u64>>,
}

pub fn run() {
    let mtm = MainThreadMarker::new().expect("Winlane must launch on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    let delegate = Delegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}

#[cfg(test)]
pub(crate) use delegate::tests;
