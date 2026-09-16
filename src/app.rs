use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use winlane::{tr, trf};

use crate::main_wake::MainWake;
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
use winlane::aliases::{AliasInput, AliasMatch, Aliases, AppIdentity};
use winlane::app_catalog::{InstalledApp, matching_apps};
use winlane::config::{ApplicationTarget, Config, visible_matches};
use winlane::displays::{Display, Rect, placements};
use winlane::search::WindowInfo;
use winlane::shortcuts::{
    Action, ActionKind, PanelCommand, PanelMode, SPACE, SwitchSelection, panel_command,
    space_changes_mode,
};

use crate::accessibility;
use crate::app_shortcuts::{self, AppShortcutsWindow};
use crate::input_source::{self, Source};
use crate::installed_apps;
use crate::settings::{self, SettingsWindow};
use crate::shortcut_tap::ShortcutTap;
use winlane::input_method::InputSession;

const WIDTH: f64 = 700.0;
const HEIGHT: f64 = 590.0;
const ROW_HEIGHT: f64 = 28.0;
const LIST_TOP: f64 = 526.0;
const LIST_BOTTOM: f64 = 36.0;
const LIST_WIDTH: f64 = WIDTH - 20.0;
const APP_CATALOG_TTL: Duration = Duration::from_secs(10 * 60);

struct PanelUi {
    display_id: u32,
    shortcut_label: Retained<NSTextField>,
    panel: Retained<SearchPanel>,
    backdrop: Retained<NSVisualEffectView>,
    input: Retained<NSSearchField>,
    scroll: Retained<NSScrollView>,
    list: Retained<ListView>,
    footer: Retained<NSTextField>,
    help: Retained<NSButton>,
    demo_button: Retained<NSButton>,
    refresh_button: Retained<NSButton>,
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
    Window(WindowInfo, Option<String>),
    Application(ApplicationTarget),
}

enum SelectedResult {
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

impl RowUi {
    fn select(&mut self, selected: bool) {
        if self.selected == Some(selected) {
            return;
        }
        self.selected = Some(selected);
        self.button.setAccessibilitySelected(selected);
        self.button.ivars().selected.set(selected);
        self.button
            .ivars()
            .has_alias
            .set(!self.alias.stringValue().is_empty());
        NSView::setNeedsDisplay(&self.button, true);
        let text = if selected {
            NSColor::whiteColor()
        } else {
            NSColor::labelColor()
        };
        let launching = matches!(self.content, Some(RowContent::Application(_)));
        let detail = if launching && !selected {
            NSColor::secondaryLabelColor()
        } else {
            text.clone()
        };
        self.title.setTextColor(Some(&detail));
        self.app.setTextColor(Some(&text));
        self.app.setAlphaValue(if selected { 1.0 } else { 0.80 });
        let alias = if selected {
            text
        } else if launching {
            NSColor::systemTealColor()
        } else {
            NSColor::systemBlueColor()
        };
        self.alias.setTextColor(Some(&alias));
    }
}

#[derive(Default)]
struct AppState {
    panels: RefCell<Vec<Rc<PanelUi>>>,
    query: RefCell<String>,
    current_app_only: Cell<bool>,
    keyboard_display: Cell<Option<u32>>,
    changing_displays: Cell<bool>,
    syncing_controls: Cell<bool>,
    saving_settings: Cell<bool>,
    input_session: RefCell<InputSession>,
    changing_input_source: Cell<bool>,
    check_panel_focus: Cell<bool>,
    config: RefCell<Config>,
    config_store: OnceCell<Retained<NSUserDefaults>>,
    aliases: RefCell<Aliases>,
    automatic_aliases: RefCell<Aliases>,
    identities: RefCell<HashMap<i32, AppIdentity>>,
    icons: RefCell<HashMap<i32, Option<Retained<NSImage>>>>,
    alias_input: RefCell<AliasInput>,
    alias_error: RefCell<Option<String>>,
    aliases_writable: Cell<bool>,
    settings: RefCell<Option<Rc<SettingsWindow>>>,
    app_shortcuts: RefCell<Option<Rc<AppShortcutsWindow>>>,
    alias_rules: RefCell<Option<Rc<crate::alias_rules::AliasRulesWindow>>>,
    launch_receiver: RefCell<Option<PendingLaunch>>,
    installed_apps: RefCell<Vec<InstalledApp>>,
    catalog_receiver: RefCell<Option<Receiver<Vec<InstalledApp>>>>,
    catalog_checked: Cell<Option<Instant>>,
    application_icons: RefCell<HashMap<String, Retained<NSImage>>>,
    show_menu_item: RefCell<Option<Retained<NSMenuItem>>>,
    status_item: OnceCell<Retained<NSStatusItem>>,
    timer: OnceCell<Retained<NSTimer>>,
    shortcut_tap: RefCell<Option<ShortcutTap>>,
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
    selected: Cell<usize>,
    previous_pid: Cell<i32>,
    previous_window: Cell<Option<u64>>,
    receiver: RefCell<Option<Receiver<Vec<WindowInfo>>>>,
    loading: Cell<bool>,
    demo: Cell<bool>,
    recency: RefCell<Vec<u64>>,
    focus_observer: RefCell<Option<crate::focus_observer::FocusObserver>>,
    preferences: RefCell<HashMap<String, u64>>,
}

define_class!(
    // SAFETY: This panel and its delegate are created and used only on the main thread.
    #[unsafe(super = NSPanel)]
    #[thread_kind = MainThreadOnly]
    #[derive(Debug)]
    struct SearchPanel;
    unsafe impl NSObjectProtocol for SearchPanel {}
    impl SearchPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool { true }
        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main(&self) -> bool { false }
        #[unsafe(method(sendEvent:))]
        fn send_event(&self, event: &NSEvent) {
            if event.r#type() == NSEventType::KeyDown {
                let composing = self.firstResponder()
                    .and_then(|responder| responder.downcast::<NSTextView>().ok())
                    .is_some_and(|editor| NSTextInputClient::hasMarkedText(&*editor));
                if i64::from(event.keyCode()) == SPACE {
                    let delegate: Option<Retained<Delegate>> = unsafe { msg_send![self, delegate] };
                    if let Some(delegate) = delegate {
                        let mode = delegate.ivars().mode.get().unwrap_or(PanelMode::Search);
                        let empty = delegate.ivars().query.borrow().is_empty();
                        if space_changes_mode(mode, empty, composing) {
                            if !event.isARepeat() { delegate.toggle_mode(event.modifierFlags().bits() as u64); }
                            return;
                        }
                    }
                }
                if let Some(command) = panel_command(i64::from(event.keyCode()), event.modifierFlags().bits() as u64, composing) {
                    // SAFETY: build_ui installs our retained Delegate as this panel's sole delegate.
                    let delegate: Option<Retained<Delegate>> = unsafe { msg_send![self, delegate] };
                    if let Some(delegate) = delegate {
                        match command {
                            PanelCommand::Next => delegate.move_selection(1),
                            PanelCommand::Previous => delegate.move_selection(-1),
                            PanelCommand::Accept => delegate.activate_selected(),
                            PanelCommand::Cancel => delegate.dismiss(),
                        }
                        return;
                    }
                }
                if !composing && event.modifierFlags().contains(NSEventModifierFlags::Command)
                    && NSApplication::sharedApplication(self.mtm()).mainMenu()
                        .is_some_and(|menu| menu.performKeyEquivalent(event))
                { return; }
            }
            // SAFETY: Unhandled events, including IME composition, follow NSPanel's normal dispatch.
            unsafe { let _: () = msg_send![super(self), sendEvent: event]; }
        }
    }
);

define_class!(
    // SAFETY: NSView has no extra subclass requirements. Coordinates are flipped for list rows.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[derive(Debug)]
    struct ListView;
    unsafe impl NSObjectProtocol for ListView {}
    impl ListView {
        #[unsafe(method(isFlipped))]
        fn flipped(&self) -> bool { true }
    }
);

define_class!(
    // SAFETY: Drawing and mouse tracking stay on AppKit's main thread.
    #[unsafe(super = NSButton)]
    #[thread_kind = MainThreadOnly]
    #[ivars = RowAppearance]
    #[derive(Debug)]
    struct WindowRowButton;
    unsafe impl NSObjectProtocol for WindowRowButton {}
    impl WindowRowButton {
        #[unsafe(method(drawRect:))]
        fn draw(&self, _: NSRect) {
            let selected = self.ivars().selected.get();
            let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(self.bounds(), 7.0, 7.0);
            if selected {
                selection_gradient().drawInBezierPath_angle(&path, 0.0);
            } else if self.ivars().hovered.get() || self.isHighlighted() {
                NSColor::systemBlueColor().colorWithAlphaComponent(0.09).setFill();
                path.fill();
            }
            if self.ivars().has_alias.get() {
                let chip = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                    rect(8.0, 4.0, 28.0, 18.0), 5.0, 5.0,
                );
                if selected {
                    NSColor::whiteColor().colorWithAlphaComponent(0.19).setFill();
                } else {
                    NSColor::systemBlueColor().colorWithAlphaComponent(0.09).setFill();
                }
                chip.fill();
            }
        }
        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking(&self) {
            if let Some(area) = self.ivars().tracking.borrow_mut().take() { self.removeTrackingArea(&area); }
            // SAFETY: The tracking area belongs to this view, which implements both mouse callbacks.
            unsafe {
                let _: () = msg_send![super(self), updateTrackingAreas];
                let area = NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(), self.bounds(),
                    NSTrackingAreaOptions::MouseEnteredAndExited | NSTrackingAreaOptions::ActiveAlways | NSTrackingAreaOptions::InVisibleRect,
                    Some(self), None,
                );
                self.addTrackingArea(&area);
                self.ivars().tracking.replace(Some(area));
            }
        }
        #[unsafe(method(mouseEntered:))]
        fn entered(&self, _: &NSEvent) { self.ivars().hovered.set(true); NSView::setNeedsDisplay(self, true); }
        #[unsafe(method(mouseExited:))]
        fn exited(&self, _: &NSEvent) { self.ivars().hovered.set(false); NSView::setNeedsDisplay(self, true); }
        #[unsafe(method(viewDidMoveToWindow))]
        fn moved_to_window(&self) {
            unsafe { let _: () = msg_send![super(self), viewDidMoveToWindow]; }
            self.ivars().hovered.set(false);
        }
        #[unsafe(method(resetCursorRects))]
        fn reset_cursor_rects(&self) {
            // SAFETY: Preserve NSButton's cursor setup before adding the row's cursor.
            unsafe { let _: () = msg_send![super(self), resetCursorRects]; }
            if self.isEnabled() {
                self.addCursorRect_cursor(self.visibleRect(), &NSCursor::pointingHandCursor());
            }
        }
    }
);

#[derive(Debug, Default)]
struct RowAppearance {
    selected: Cell<bool>,
    hovered: Cell<bool>,
    has_alias: Cell<bool>,
    tracking: RefCell<Option<Retained<NSTrackingArea>>>,
}

define_class!(
    // SAFETY: This view draws a static tint above AppKit's native material on the main thread.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    struct PanelSurface;
    unsafe impl NSObjectProtocol for PanelSurface {}
    impl PanelSurface {
        #[unsafe(method(drawRect:))]
        fn draw(&self, dirty: NSRect) {
            unsafe { let _: () = msg_send![super(self), drawRect: dirty]; }
            let dark = unsafe {
                self.effectiveAppearance().bestMatchFromAppearancesWithNames(
                    &NSArray::from_slice(&[NSAppearanceNameAqua, NSAppearanceNameDarkAqua]),
                ).is_some_and(|name| &*name == NSAppearanceNameDarkAqua)
            };
            let (start, end) = if dark {
                (tint(0x172039, 0.88), tint(0x102b32, 0.80))
            } else {
                (tint(0xf5f7ff, 0.94), tint(0xecf8fa, 0.88))
            };
            if let Some(gradient) = NSGradient::initWithStartingColor_endingColor(NSGradient::alloc(), &start, &end) {
                gradient.drawInRect_angle(self.bounds(), -25.0);
            }
        }
    }
);

define_class!(
    // SAFETY: All AppKit state is main-thread-only and lives until the application terminates.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = AppState]
    struct Delegate;
    unsafe impl NSObjectProtocol for Delegate {}
    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidBecomeActive:))]
        fn became_active(&self, _: &NSNotification) {
            self.check_language();
            if let Some(settings) = self.settings_window() { settings.update_login_status(); }
        }
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_launch(&self, _: &NSNotification) {
            settings::apply_language(winlane::i18n::Language::System);
            match settings::load() {
                Ok(config) => { self.ivars().config.replace(config); }
                Err(error) => { self.ivars().hotkey_error.replace(Some(error)); }
            }
            settings::apply_language(self.ivars().config.borrow().language);
            match settings::load_aliases() {
                Ok(aliases) => {
                    self.ivars().aliases.replace(aliases.with_rules(&[], &HashMap::new(), &self.ivars().config.borrow().alias_rules));
                    self.ivars().automatic_aliases.replace(aliases);
                    self.ivars().aliases_writable.set(true);
                }
                Err(error) => { self.ivars().alias_error.replace(Some(error)); }
            }
            settings::apply_appearance(&self.ivars().config.borrow(), self.mtm());
            self.build_ui();
            self.register_hotkeys();
            if !accessibility::is_trusted() { self.show(); } else { self.refresh(); }
        }
        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn reopen(&self, _: &NSApplication, _: bool) -> bool { self.show(); true }
        #[unsafe(method(applicationDidChangeScreenParameters:))]
        fn screens_changed(&self, _: &NSNotification) {
            if !self.ivars().panels.borrow().is_empty() {
                self.sync_displays();
                self.render();
                if self.ivars().mode.get().is_some() && self.ivars().switch_timer.borrow().is_none() { self.present_panels(); }
            }
        }
    }
    unsafe impl NSWindowDelegate for Delegate {
        #[unsafe(method(windowDidBecomeKey:))]
        fn became_key(&self, notification: &NSNotification) {
            if notification.object().and_then(|object| object.downcast::<SearchPanel>().ok()).is_none() { return; }
            self.ivars().check_panel_focus.set(false);
            self.focus_search();
        }
        #[unsafe(method(windowShouldClose:))]
        fn should_close(&self, window: &NSWindow) -> bool {
            if window.downcast_ref::<SearchPanel>().is_some() {
                self.dismiss(); false
            } else {
                window.makeFirstResponder(None);
                true
            }
        }
        #[unsafe(method(windowDidResignKey:))]
        fn resigned(&self, notification: &NSNotification) {
            let Some(window) = notification.object().and_then(|object| object.downcast::<NSWindow>().ok()) else { return; };
            if window.downcast_ref::<SearchPanel>().is_none() {
                window.makeFirstResponder(None);
                return;
            }
            self.ivars().check_panel_focus.set(true);
            self.ivars().wake.get().unwrap().signal();
        }
    }
    unsafe impl NSControlTextEditingDelegate for Delegate {
        #[unsafe(method(controlTextDidChange:))]
        fn text_changed(&self, notification: &NSNotification) {
            if self.ivars().syncing_controls.get() { return; }
            if let Some(input) = notification.object().and_then(|object| object.downcast::<NSSearchField>().ok()) {
                self.ivars().query.replace(input.stringValue().to_string());
                self.filter();
            }
        }
        #[unsafe(method(control:textView:doCommandBySelector:))]
        fn text_command(&self, _: &NSControl, editor: &NSTextView, command: Sel) -> bool {
            if NSTextInputClient::hasMarkedText(editor) { false }
            else if command == sel!(moveDown:) || command == sel!(insertTab:) {
                self.move_selection(1); true
            } else if command == sel!(moveUp:) || command == sel!(insertBacktab:) {
                self.move_selection(-1); true
            } else if command == sel!(insertNewline:) {
                self.activate_selected(); true
            } else if command == sel!(cancelOperation:) {
                self.dismiss(); true
            } else { false }
        }
    }
    unsafe impl NSTextFieldDelegate for Delegate {}
    unsafe impl NSSearchFieldDelegate for Delegate {}
    unsafe impl NSMenuItemValidation for Delegate {
        #[unsafe(method(validateMenuItem:))]
        fn validate_menu_item(&self, item: &NSMenuItem) -> bool {
            let action = item.action();
            if action == Some(sel!(toggleScope:)) {
                item.setState(if self.ivars().current_app_only.get() { NSControlStateValueOn } else { NSControlStateValueOff });
                self.any_panel_visible() && self.ivars().mode.get() == Some(PanelMode::Search)
            } else if [sel!(minimizeChosen:), sel!(hideChosen:), sel!(copyTitle:), sel!(quickSelect:)].into_iter().any(|sel| action == Some(sel)) {
                let visible = self.any_panel_visible();
                visible && if action == Some(sel!(quickSelect:)) {
                    (item.tag() as usize) < self.match_count()
                } else { self.selected_window().is_some() }
            } else { true }
        }
    }
    impl Delegate {
        #[unsafe(method(presentSwitch:))]
        fn present_switch(&self, timer: &NSTimer) {
            self.drain_shortcut_actions();
            let state = self.ivars();
            let current = state.switch_timer.borrow().as_ref().is_some_and(|pending| std::ptr::eq(&**pending, timer));
            if current && state.mode.get() == Some(PanelMode::Switch)
                && !state.switch_selection.borrow().as_ref().is_some_and(SwitchSelection::released)
            { self.present_panels(); }
        }
        #[unsafe(method(workspaceActivated:))]
        fn workspace_activated(&self, _: &NSNotification) { self.track_frontmost(); }
        #[unsafe(method(inputSourceChanged:))]
        fn input_source_changed(&self, _: &NSNotification) {
            if !self.ivars().changing_input_source.get() {
                self.remember_search_input();
            }
        }
        #[unsafe(method(showSettings:))]
        fn settings_action(&self, _: Option<&AnyObject>) {
            self.ivars().launch_receiver.replace(None);
            self.cancel_routing();
            self.end_session();
            let settings = self.ensure_settings_window();
            NSApplication::sharedApplication(self.mtm()).activate();
            settings.show(&self.ivars().config.borrow());
            self.report_shortcut_status();
        }
        #[unsafe(method(settingsChanged:))]
        fn settings_changed(&self, _: Option<&AnyObject>) { self.autosave_settings(); }
        #[unsafe(method(resetSettings:))]
        fn reset_settings(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.settings_window() {
                if self.ivars().saving_settings.replace(true) { return; }
                let result = self.apply_config(Config::default());
                self.ivars().saving_settings.set(false);
                match result {
                    Ok(()) => {
                        if let Some(settings) = self.settings_window() {
                            settings.fill(&self.ivars().config.borrow());
                            settings.report(tr!("默认设置已恢复并保存。登录启动状态保持不变。", "Defaults restored and saved. Launch at login is unchanged."), false);
                        }
                        if let Some(window) = self.app_shortcuts_window() {
                            window.fill(&self.ivars().config.borrow().app_shortcuts, self, self.mtm());
                        }
                        if let Some(window) = self.ivars().alias_rules.borrow().clone() {
                            window.fill(&self.ivars().config.borrow().alias_rules, self, self.mtm());
                        }
                    }
                    Err(error) => settings.report(&trf!("未保存：{}", "Not saved: {}", error), true),
                }
            }
        }
        #[unsafe(method(changeBackgroundOpacity:))]
        fn change_background_opacity(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.settings_window() {
                settings.opacity_slider_changed();
                self.autosave_settings();
            }
        }
        #[unsafe(method(commitBackgroundOpacity:))]
        fn commit_background_opacity(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.settings_window() {
                match settings.opacity_input_changed() {
                    Ok(()) => self.autosave_settings(),
                    Err(error) => settings.report(&trf!("未保存：{}", "Not saved: {}", error), true),
                }
            }
        }
        #[unsafe(method(showAliasRules:))]
        fn show_alias_rules(&self, _: Option<&AnyObject>) {
            self.cancel_routing(); self.end_session();
            let window = self.ensure_alias_rules_window();
            window.fill(&self.ivars().config.borrow().alias_rules, self, self.mtm());
            NSApplication::sharedApplication(self.mtm()).activate();
            window.window.center(); window.window.makeKeyAndOrderFront(None);
        }
        #[unsafe(method(addAliasRule:))]
        fn add_alias_rule(&self, _: Option<&AnyObject>) {
            self.ensure_alias_rules_window().add(self, self.mtm());
        }
        #[unsafe(method(removeAliasRule:))]
        fn remove_alias_rule(&self, sender: &NSButton) {
            self.ensure_alias_rules_window().remove(sender.tag() as usize);
            self.autosave_alias_rules();
        }
        #[unsafe(method(chooseAliasApp:))]
        fn choose_alias_app(&self, sender: &NSButton) {
            self.ensure_alias_rules_window().choose(sender.tag() as usize, self.mtm());
        }
        #[unsafe(method(aliasRulesChanged:))]
        fn alias_rules_changed(&self, _: Option<&AnyObject>) { self.autosave_alias_rules(); }
        #[unsafe(method(showAppShortcuts:))]
        fn app_shortcuts_action(&self, _: Option<&AnyObject>) {
            self.show_app_shortcuts();
        }
        #[unsafe(method(addAppShortcut:))]
        fn add_app_shortcut(&self, _: Option<&AnyObject>) {
            if let Some(window) = self.app_shortcuts_window() { window.add(self, self.mtm()); }
        }
        #[unsafe(method(removeAppShortcut:))]
        fn remove_app_shortcut(&self, sender: &NSButton) {
            if let Some(window) = self.app_shortcuts_window() {
                window.remove(sender.tag() as usize);
                self.autosave_app_shortcuts();
            }
        }
        #[unsafe(method(chooseShortcutApp:))]
        fn choose_shortcut_app(&self, sender: &NSButton) {
            if let Some(window) = self.app_shortcuts_window() { window.choose(sender.tag() as usize, self.mtm()); }
        }
        #[unsafe(method(appShortcutsChanged:))]
        fn app_shortcuts_changed(&self, _: Option<&AnyObject>) { self.autosave_app_shortcuts(); }
        #[unsafe(method(toggleLogin:))]
        fn toggle_login(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.settings_window() { settings.toggle_login(); }
        }
        #[unsafe(method(manageLogin:))]
        fn manage_login(&self, _: Option<&AnyObject>) { settings::manage_login(); }
        #[unsafe(method(toggleScope:))]
        fn toggle_scope(&self, _: Option<&AnyObject>) {
            self.ivars().current_app_only.set(!self.ivars().current_app_only.get());
            self.filter();
        }
        #[unsafe(method(minimizeChosen:))]
        fn minimize_chosen(&self, _: Option<&AnyObject>) { self.minimize_selected(); }
        #[unsafe(method(hideChosen:))]
        fn hide_chosen(&self, _: Option<&AnyObject>) { self.hide_selected(); }
        #[unsafe(method(copyTitle:))]
        fn copy_title(&self, _: Option<&AnyObject>) {
            if !self.any_panel_visible() { return; }
            if let Some(window) = self.selected_window() {
                let pasteboard = NSPasteboard::generalPasteboard();
                pasteboard.clearContents();
                // SAFETY: AppKit exports this immutable pasteboard type on all supported systems.
                if pasteboard.setString_forType(&NSString::from_str(&window.title), unsafe { NSPasteboardTypeString }) {
                    self.report_switch_error(tr!("已复制窗口标题。", "Window title copied."));
                } else { self.report_switch_error(tr!("无法写入剪贴板，请重试。", "Could not copy to the clipboard. Try again.")); }
            }
        }
        #[unsafe(method(quickSelect:))]
        fn quick_select(&self, sender: &NSMenuItem) {
            if !self.any_panel_visible() { return; }
            if (sender.tag() as usize) < self.match_count() {
                self.ivars().selected.set(sender.tag() as usize);
                self.render(); self.activate_selected();
            }
        }
        #[unsafe(method(showSearch:))]
        fn show_action(&self, _: Option<&AnyObject>) { self.show(); }
        #[unsafe(method(showSwitcher:))]
        fn switch_action(&self, _: Option<&AnyObject>) { self.open_switcher(); }
        #[unsafe(method(closeWindow:))]
        fn close_window(&self, _: Option<&AnyObject>) {
            let alias_rules = self.ivars().alias_rules.borrow().clone();
            if let Some(window) = alias_rules && window.window.isKeyWindow() {
                window.window.makeFirstResponder(None); window.window.close();
            } else if let Some(settings) = self.settings_window() && settings.window.isKeyWindow() {
                settings.window.makeFirstResponder(None);
                settings.window.close();
            } else if self.any_panel_key() { self.dismiss(); }
        }
        #[unsafe(method(refreshWindows:))]
        fn refresh_action(&self, _: Option<&AnyObject>) {
            if self.ivars().demo.get() { self.filter(); } else {
                self.ivars().catalog_checked.set(None);
                self.ensure_app_catalog();
                self.refresh();
            }
        }
        #[unsafe(method(pickWindow:))]
        fn pick(&self, sender: &NSButton) {
            self.ivars().selected.set(sender.tag() as usize);
            self.render();
            self.activate_selected();
        }
        #[unsafe(method(openPermissions:))]
        fn permissions(&self, _: Option<&AnyObject>) {
            accessibility::request_permission();
            if let Some(url) = NSURL::URLWithString(ns_string!("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")) {
                NSWorkspace::sharedWorkspace().openURL(&url);
            }
        }
        #[unsafe(method(toggleDemo:))]
        fn toggle_demo(&self, _: Option<&AnyObject>) {
            let state = self.ivars();
            state.demo.set(!state.demo.get());
            state.query.borrow_mut().clear();
            if state.demo.get() {
                state.windows.replace(demo_windows());
                state.loading.set(false);
                self.filter();
            } else { state.windows.borrow_mut().clear(); self.refresh(); }
        }
        #[unsafe(method(poll:))]
        fn poll(&self, _: Option<&AnyObject>) {
            if self.ivars().check_panel_focus.replace(false)
                && !self.ivars().changing_displays.get()
                && !self.any_panel_key()
            { self.end_session(); }
            self.check_shortcuts();
            self.drain_shortcut_actions();
            self.poll_app_launch();
            self.poll_app_catalog();
            let result = self.ivars().receiver.borrow().as_ref().map(|rx| rx.try_recv());
            if let Some(Ok(mut windows)) = result {
                self.ivars().receiver.replace(None);
                self.ivars().loading.set(false);
                if !self.ivars().demo.get() {
                    if !accessibility::is_trusted() { windows.clear(); }
                    let snapshot_ready = self.ivars().switch_selection.borrow().as_ref()
                        .is_some_and(|selection| selection.selected().is_some());
                    if self.ivars().mode.get() == Some(PanelMode::Switch) && snapshot_ready {
                        self.ivars().deferred_windows.replace(Some(windows));
                        self.select_alias();
                        self.render();
                        self.commit_switch_if_ready();
                    } else {
                        let selected_id = self.selected_result();
                        self.install_windows(windows);
                        self.filter_preserving(selected_id);
                        self.prepare_switch_selection();
                        self.commit_switch_if_ready();
                    }
                }
            } else if matches!(result, Some(Err(TryRecvError::Disconnected))) {
                self.ivars().receiver.replace(None);
                self.ivars().loading.set(false);
                self.render();
                if self.ivars().mode.get().is_some() { self.switch_to_search(); }
                self.report_switch_error(tr!("读取窗口失败，请按 ⌘R 重试。", "Could not read windows. Press ⌘R to retry."));
            }
        }
        #[unsafe(method(quitApp:))]
        fn quit(&self, _: Option<&AnyObject>) {
            let alias_rules = self.ivars().alias_rules.borrow().clone();
            if let Some(window) = alias_rules { window.window.makeFirstResponder(None); }
            if let Some(settings) = self.settings_window() {
                settings.window.makeFirstResponder(None);
            }
            self.end_session();
            NSApplication::sharedApplication(self.mtm()).terminate(None);
        }
    }
);

impl Delegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(AppState::default());
        // SAFETY: NSObject's initializer has this exact signature.
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        let weak = Weak::new(&*this);
        let wake = MainWake::new(mtm, move || {
            if let Some(delegate) = weak.load() {
                delegate.poll(sel!(poll:), None);
            }
        });
        assert!(this.ivars().wake.set(wake).is_ok());
        this
    }

    fn build_ui(&self) {
        let mtm = self.mtm();
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        self.build_menus();
        // SAFETY: The delegate lives for the app lifetime; AppKit delivers this notification on the main thread.
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                self,
                sel!(inputSourceChanged:),
                Some(NSTextInputContextKeyboardSelectionDidChangeNotification),
                None,
            );
        }
        // SAFETY: Workspace delivers application activation notifications on the main thread.
        unsafe {
            NSWorkspace::sharedWorkspace()
                .notificationCenter()
                .addObserver_selector_name_object(
                    self,
                    sel!(workspaceActivated:),
                    Some(NSWorkspaceDidActivateApplicationNotification),
                    None,
                );
        }
        self.track_frontmost();
        // SAFETY: The application retains this delegate for the entire run loop; poll: has NSTimer signature.
        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                1.0,
                self,
                sel!(poll:),
                None,
                true,
            )
        };
        timer.setTolerance(0.1);
        // SAFETY: Permission and event-tap health checks also run during menu tracking.
        // Keyboard and scan results wake the run loop independently of this timer.
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
        self.ivars().timer.set(timer).unwrap();
    }

    fn build_menus(&self) {
        let mtm = self.mtm();
        let app = NSApplication::sharedApplication(mtm);
        let main_menu = NSMenu::new(mtm);
        let application_item = NSMenuItem::new(mtm);
        let application_menu = NSMenu::new(mtm);
        // SAFETY: quitApp: is implemented by this retained delegate.
        let quit_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(tr!("退出 Winlane", "Quit Winlane")),
                Some(sel!(quitApp:)),
                ns_string!("q"),
            )
        };
        unsafe { quit_item.setTarget(Some(self)) };
        application_menu.addItem(&quit_item);
        let settings_item = self.menu_item(tr!("设置…", "Settings…"), sel!(showSettings:), ",");
        application_menu.insertItem_atIndex(&settings_item, 0);
        application_menu.insertItem_atIndex(
            &self.menu_item(
                tr!("打开窗口搜索", "Open Window Search"),
                sel!(showSearch:),
                "",
            ),
            0,
        );
        application_item.setSubmenu(Some(&application_menu));
        main_menu.addItem(&application_item);
        let edit_menu = NSMenu::new(mtm);
        let edit_item = NSMenuItem::new(mtm);
        edit_item.setTitle(&NSString::from_str(tr!("编辑", "Edit")));
        for (title, action, key) in [
            (tr!("剪切", "Cut"), sel!(cut:), "x"),
            (tr!("拷贝", "Copy"), sel!(copy:), "c"),
            (tr!("粘贴", "Paste"), sel!(paste:), "v"),
            (tr!("全选", "Select All"), sel!(selectAll:), "a"),
        ] {
            // SAFETY: Standard editing actions are resolved by AppKit's responder chain.
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(title),
                    Some(action),
                    &NSString::from_str(key),
                )
            };
            edit_menu.addItem(&item);
        }
        edit_item.setSubmenu(Some(&edit_menu));
        main_menu.addItem(&edit_item);
        let window_item = NSMenuItem::new(mtm);
        window_item.setTitle(&NSString::from_str(tr!("窗口", "Window")));
        let window_menu = NSMenu::new(mtm);
        window_menu.addItem(&self.menu_item(
            tr!("关闭窗口", "Close Window"),
            sel!(closeWindow:),
            "w",
        ));
        window_menu.addItem(&NSMenuItem::separatorItem(mtm));
        window_menu.addItem(&self.menu_item(
            tr!("仅当前应用", "Current app only"),
            sel!(toggleScope:),
            "",
        ));
        window_menu.addItem(&NSMenuItem::separatorItem(mtm));
        self.add_window_actions(&window_menu);
        window_menu.addItem(&NSMenuItem::separatorItem(mtm));
        window_menu.addItem(&self.menu_item(
            tr!("刷新窗口", "Refresh Windows"),
            sel!(refreshWindows:),
            "r",
        ));
        for index in 0..9 {
            let item = self.menu_item(
                &trf!("切换到第 {} 个窗口", "Switch to Window {}", index + 1),
                sel!(quickSelect:),
                &(index + 1).to_string(),
            );
            item.setTag(index);
            window_menu.addItem(&item);
        }
        window_item.setSubmenu(Some(&window_menu));
        main_menu.addItem(&window_item);
        app.setMainMenu(Some(&main_menu));

        let status_item = self.ivars().status_item.get_or_init(|| {
            NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength)
        });
        if let Some(button) = status_item.button(mtm) {
            let data = NSData::with_bytes(include_bytes!("../resources/MenuBarIconTemplate.pdf"));
            let icon = NSImage::initWithData(NSImage::alloc(), &data)
                .expect("the embedded menu-bar icon must be a valid PDF");
            icon.setSize(NSSize::new(18.0, 18.0));
            icon.setTemplate(true);
            button.setTitle(ns_string!(""));
            button.setImage(Some(&icon));
            button.setImagePosition(NSCellImagePosition::ImageOnly);
            button.setAccessibilityLabel(Some(ns_string!("Winlane")));
            button.setToolTip(Some(&NSString::from_str(tr!(
                "Winlane · 窗口搜索",
                "Winlane · Window Search"
            ))));
        }
        let menu = NSMenu::new(mtm);
        for (title, action, key) in [
            (
                tr!("打开窗口搜索", "Open Window Search"),
                sel!(showSearch:),
                "",
            ),
            (
                tr!("打开窗口切换", "Open Window Switcher"),
                sel!(showSwitcher:),
                "",
            ),
            (tr!("设置…", "Settings…"), sel!(showSettings:), ","),
            (
                tr!("刷新窗口列表", "Refresh Window List"),
                sel!(refreshWindows:),
                "",
            ),
            (
                tr!("辅助功能设置…", "Accessibility Settings…"),
                sel!(openPermissions:),
                "",
            ),
            (tr!("退出 Winlane", "Quit Winlane"), sel!(quitApp:), "q"),
        ] {
            // SAFETY: These selectors belong to this retained delegate and accept one object argument.
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(title),
                    Some(action),
                    &NSString::from_str(key),
                )
            };
            unsafe { item.setTarget(Some(self)) };
            menu.addItem(&item);
            if action == sel!(showSearch:) {
                self.ivars().show_menu_item.replace(Some(item));
            }
        }
        status_item.setMenu(Some(&menu));
        self.update_shortcut_labels();
    }

    fn settings_window(&self) -> Option<Rc<SettingsWindow>> {
        self.ivars().settings.borrow().clone()
    }

    fn ensure_settings_window(&self) -> Rc<SettingsWindow> {
        if let Some(window) = self.settings_window() {
            return window;
        }
        let window = Rc::new(SettingsWindow::new(self, self.mtm()));
        window
            .window
            .setDelegate(Some(ProtocolObject::from_ref(self)));
        self.ivars().settings.replace(Some(window.clone()));
        window
    }

    fn ensure_alias_rules_window(&self) -> Rc<crate::alias_rules::AliasRulesWindow> {
        if let Some(window) = self.ivars().alias_rules.borrow().clone() {
            return window;
        }
        let window = Rc::new(crate::alias_rules::AliasRulesWindow::new(self, self.mtm()));
        window
            .window
            .setDelegate(Some(ProtocolObject::from_ref(self)));
        self.ivars().alias_rules.replace(Some(window.clone()));
        window
    }

    fn autosave_alias_rules(&self) {
        let Some(window) = self.ivars().alias_rules.borrow().clone() else {
            return;
        };
        let result = window.candidate().and_then(|rules| {
            let mut config = self.ivars().config.borrow().clone();
            config.alias_rules = rules;
            self.apply_config(config)
        });
        match result {
            Ok(()) => window.report(
                tr!(
                    "完整规则已自动保存；未填完的行暂不启用。",
                    "Complete rules saved automatically; unfinished rows stay inactive."
                ),
                false,
            ),
            Err(error) => window.report(&trf!("未保存：{}", "Not saved: {}", error), true),
        }
    }

    fn app_shortcuts_window(&self) -> Option<Rc<AppShortcutsWindow>> {
        self.ivars().app_shortcuts.borrow().clone()
    }

    fn ensure_app_shortcuts_window(&self) -> Rc<AppShortcutsWindow> {
        if let Some(window) = self.app_shortcuts_window() {
            return window;
        }
        let window = Rc::new(AppShortcutsWindow::new(self, self.mtm()));
        self.ivars().app_shortcuts.replace(Some(window.clone()));
        window
    }

    fn check_language(&self) {
        let changed = settings::apply_language(self.ivars().config.borrow().language);
        if changed && self.ivars().status_item.get().is_some() {
            self.rebuild_localized_ui();
        }
    }

    fn rebuild_localized_ui(&self) {
        let state = self.ivars();
        self.build_menus();
        let visible = self.any_panel_visible();
        state.changing_displays.set(true);
        for ui in state.panels.take() {
            ui.panel.setDelegate(None);
            ui.panel.orderOut(None);
            ui.panel.close();
        }
        state.changing_displays.set(false);
        self.sync_displays();
        self.render();
        if visible {
            self.present_panels();
        }
        let old_settings = state.settings.take();
        if let Some(old) = old_settings {
            let showing = old.window.isVisible();
            let frame = old.window.frame();
            let tab = old.selected_tab();
            let candidate = old
                .candidate()
                .unwrap_or_else(|_| state.config.borrow().clone());
            old.window.close();
            if showing {
                let window = self.ensure_settings_window();
                window.show(&candidate);
                window.select_tab(tab);
                window.window.setFrameOrigin(frame.origin);
                self.report_shortcut_status();
            }
        }
        let old_alias_rules = state.alias_rules.take();
        if let Some(old) = old_alias_rules {
            let showing = old.window.isVisible();
            let frame = old.window.frame();
            old.window.close();
            if showing {
                let window = self.ensure_alias_rules_window();
                window.copy_draft_from(&old, self, self.mtm());
                window.window.setFrameOrigin(frame.origin);
                window.window.makeKeyAndOrderFront(None);
            }
        }
        let old_shortcuts = state.app_shortcuts.take();
        if let Some(old) = old_shortcuts {
            let showing = old.window.isVisible();
            let frame = old.window.frame();
            old.window.close();
            if showing {
                let window = self.ensure_app_shortcuts_window();
                window.show(&[], self, self.mtm());
                window.copy_draft_from(&old, self, self.mtm());
                window.window.setFrameOrigin(frame.origin);
            }
        }
    }

    fn panels(&self) -> Vec<Rc<PanelUi>> {
        self.ivars().panels.borrow().clone()
    }

    fn any_panel_visible(&self) -> bool {
        self.panels().iter().any(|ui| ui.panel.isVisible())
    }

    fn any_panel_key(&self) -> bool {
        self.panels().iter().any(|ui| ui.panel.isKeyWindow())
    }

    fn sync_displays(&self) {
        let state = self.ivars();
        state.changing_displays.set(true);
        let syncing_controls = state.syncing_controls.replace(true);
        let existing = self.panels();
        let focused = existing
            .iter()
            .find(|ui| ui.panel.isKeyWindow())
            .map(|ui| ui.display_id);
        let displays: Vec<_> = NSScreen::screens(self.mtm())
            .iter()
            .enumerate()
            .map(|(index, screen)| {
                let id = screen
                    .deviceDescription()
                    .objectForKey(ns_string!("NSScreenNumber"))
                    .and_then(|value| value.downcast::<NSNumber>().ok())
                    .map_or(index as u32, |number| number.unsignedIntValue());
                Display {
                    id,
                    frame: display_rect(screen.frame()),
                    visible: display_rect(screen.visibleFrame()),
                }
            })
            .collect();
        let pointer = NSEvent::mouseLocation();
        let positions = placements(&displays, (WIDTH, HEIGHT), (pointer.x, pointer.y), focused);
        state.keyboard_display.set(
            positions
                .iter()
                .find(|position| position.receives_keyboard)
                .map(|position| position.display_id),
        );
        let mut panels = Vec::new();
        for position in positions {
            let ui = existing
                .iter()
                .find(|ui| ui.display_id == position.display_id)
                .cloned()
                .unwrap_or_else(|| self.create_panel(position.display_id));
            if state.mode.get().is_some() {
                self.render_panel(&ui);
            }
            let display = displays
                .iter()
                .find(|display| display.id == position.display_id)
                .unwrap();
            let y = display.visible.y
                + ((display.visible.height - ui.panel.frame().size.height) * 0.58).max(0.0);
            ui.panel.setFrameOrigin(NSPoint::new(position.x, y));
            panels.push(ui);
        }
        state.panels.replace(panels.clone());
        for ui in existing
            .iter()
            .filter(|ui| !panels.iter().any(|new| new.display_id == ui.display_id))
        {
            ui.panel.setDelegate(None);
            ui.panel.orderOut(None);
            ui.panel.close();
        }
        state.syncing_controls.set(syncing_controls);
        state.changing_displays.set(false);
    }

    fn present_panels(&self) {
        self.cancel_switch_timer();
        let state = self.ivars();
        state.changing_displays.set(true);
        let panels = self.panels();
        for ui in &panels {
            ui.panel.orderFrontRegardless();
        }
        if let Some(ui) = panels
            .iter()
            .find(|ui| Some(ui.display_id) == state.keyboard_display.get())
        {
            ui.panel.makeKeyAndOrderFront(None);
            if state.mode.get() == Some(PanelMode::Switch) {
                ui.panel.makeFirstResponder(None);
            } else {
                self.focus_search();
            }
        }
        state.changing_displays.set(false);
    }

    fn create_panel(&self, display_id: u32) -> Rc<PanelUi> {
        let mtm = self.mtm();
        // SAFETY: We own the window and disable AppKit's release-on-close behavior below.
        let panel: Retained<SearchPanel> = unsafe {
            msg_send![super(SearchPanel::alloc(mtm).set_ivars(())), initWithContentRect:
                rect(0.0, 0.0, WIDTH, HEIGHT),
                styleMask: NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::FullSizeContentView
                    | NSWindowStyleMask::NonactivatingPanel,
                backing: NSBackingStoreType::Buffered,
                defer: false]
        };
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setTitleVisibility(NSWindowTitleVisibility::Hidden);
        panel.setTitlebarAppearsTransparent(true);
        for button in [
            NSWindowButton::CloseButton,
            NSWindowButton::MiniaturizeButton,
            NSWindowButton::ZoomButton,
        ] {
            if let Some(button) = panel.standardWindowButton(button) {
                button.setHidden(true);
            }
        }
        panel.setLevel(NSFloatingWindowLevel);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::IgnoresCycle
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        panel.setDelegate(Some(ProtocolObject::from_ref(self)));
        panel.setHidesOnDeactivate(false);
        panel.setBecomesKeyOnlyIfNeeded(false);
        panel.setMovableByWindowBackground(true);
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        let root = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, WIDTH, HEIGHT));
        panel.setContentView(Some(&root));
        let backdrop = panel_backdrop(root.bounds(), mtm);
        root.addSubview(&backdrop);

        let shortcut = label(
            &self.ivars().config.borrow().shortcut.display(),
            11.0,
            rect(WIDTH - 120.0, 546.0, 104.0, 18.0),
            mtm,
        );
        shortcut.setAlignment(NSTextAlignment::Right);
        shortcut.setTextColor(Some(&NSColor::labelColor()));
        shortcut.setAlphaValue(0.65);
        root.addSubview(&shortcut);

        let input_width = ((WIDTH - 32.0) * 0.618).round();
        let input = NSSearchField::initWithFrame(
            NSSearchField::alloc(mtm),
            rect((WIDTH - input_width) / 2.0, 538.0, input_width, 34.0),
        );
        input.setFont(Some(&NSFont::systemFontOfSize(15.0)));
        input.setPlaceholderString(Some(ns_string!("")));
        input.setSendsSearchStringImmediately(true);
        input.setMaximumRecents(0);
        panel.setInitialFirstResponder(Some(&input));
        unsafe {
            input.setDelegate(Some(ProtocolObject::from_ref(self)));
            root.addSubview(&input);
        }
        let mode_label = label("", 11.0, rect(16.0, LIST_BOTTOM, WIDTH - 32.0, 20.0), mtm);
        mode_label.setTextColor(Some(&NSColor::labelColor()));
        mode_label.setAlphaValue(0.65);
        mode_label.setHidden(true);
        root.addSubview(&mode_label);

        let scroll = NSScrollView::initWithFrame(
            NSScrollView::alloc(mtm),
            rect(10.0, LIST_BOTTOM, LIST_WIDTH, LIST_TOP - LIST_BOTTOM),
        );
        scroll.setHasVerticalScroller(true);
        scroll.setScrollerStyle(NSScrollerStyle::Overlay);
        scroll.setAutohidesScrollers(true);
        scroll.setDrawsBackground(false);
        scroll.setBorderType(NSBorderType::NoBorder);
        let list: Retained<ListView> = unsafe {
            msg_send![ListView::alloc(mtm), initWithFrame: rect(0.0, 0.0, LIST_WIDTH, LIST_TOP - LIST_BOTTOM)]
        };
        scroll.setDocumentView(Some(&list));
        root.addSubview(&scroll);

        let footer = label(
            tr!("正在准备窗口列表…", "Preparing windows…"),
            11.0,
            rect(16.0, 9.0, WIDTH - 148.0, 18.0),
            mtm,
        );
        footer.setTextColor(Some(&NSColor::labelColor()));
        footer.setAlphaValue(0.65);
        root.addSubview(&footer);
        let help = self.button(
            tr!("辅助功能设置…", "Accessibility Settings…"),
            sel!(openPermissions:),
            rect(16.0, 34.0, 180.0, 25.0),
        );
        root.addSubview(&help);
        let demo_button = self.button(
            tr!("查看演示", "View Demo"),
            sel!(toggleDemo:),
            rect(204.0, 34.0, 142.0, 25.0),
        );
        root.addSubview(&demo_button);
        let refresh = self.button(
            tr!("刷新 ↻", "Refresh ↻"),
            sel!(refreshWindows:),
            rect(WIDTH - 106.0, 34.0, 90.0, 25.0),
        );
        refresh.setHidden(accessibility::is_trusted());
        root.addSubview(&refresh);
        let settings_button = self.button(
            tr!("设置…  ⌘,", "Settings…  ⌘,"),
            sel!(showSettings:),
            rect(WIDTH - 126.0, 9.0, 110.0, 24.0),
        );
        settings_button.setBordered(false);
        settings_button.setContentTintColor(Some(&NSColor::labelColor()));
        root.addSubview(&settings_button);

        Rc::new(PanelUi {
            display_id,
            panel,
            backdrop,
            input,
            scroll,
            list,
            footer,
            help,
            demo_button,
            refresh_button: refresh,
            shortcut_label: shortcut,
            mode_label,
            rows: RefCell::new(Vec::new()),
            empty_labels: RefCell::new(Vec::new()),
        })
    }

    fn button(&self, title: &str, action: Sel, frame: NSRect) -> Retained<NSButton> {
        let button = NSButton::initWithFrame(NSButton::alloc(self.mtm()), frame);
        button.setTitle(&NSString::from_str(title));
        button.setBezelStyle(NSBezelStyle::Push);
        button.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        // SAFETY: The delegate outlives the button; each action has the standard sender argument.
        unsafe {
            button.setTarget(Some(self));
            button.setAction(Some(action));
        }
        button
    }

    fn register_hotkeys(&self) {
        let result = {
            let config = self.ivars().config.borrow();
            config.validate().and_then(|()| {
                ShortcutTap::new(
                    self.mtm(),
                    config.shortcut.binding()?,
                    config.switch_shortcut.binding()?,
                    config.app_bindings()?,
                    self.ivars().wake.get().unwrap().handle(),
                )
            })
        };
        match result {
            Ok((tap, receiver)) => {
                self.ivars().shortcut_tap.replace(Some(tap));
                self.ivars().shortcut_rx.replace(Some(receiver));
                self.ivars().hotkey_error.replace(None);
            }
            Err(error) => {
                self.ivars().hotkey_error.replace(Some(error));
            }
        }
    }

    fn check_shortcuts(&self) {
        let state = self.ivars();
        if state
            .last_shortcut_check
            .get()
            .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        state.last_shortcut_check.set(Some(Instant::now()));
        if !accessibility::is_trusted() {
            state.focus_observer.replace(None);
            if state.shortcut_tap.borrow().is_some() {
                self.end_session();
                state.shortcut_tap.replace(None);
                state.shortcut_rx.replace(None);
                state.hotkey_error.replace(Some(
                    tr!(
                        "全局快捷键未启用，请在系统设置中重新允许 Winlane。",
                        "Global shortcuts are disabled. Allow Winlane again in System Settings."
                    )
                    .into(),
                ));
                self.report_shortcut_status();
            }
        } else if state.shortcut_tap.borrow().is_none() {
            self.register_hotkeys();
            self.track_frontmost();
            self.report_shortcut_status();
        } else if !state
            .shortcut_tap
            .borrow()
            .as_ref()
            .is_some_and(ShortcutTap::is_enabled)
        {
            state.hotkey_error.replace(Some(
                tr!(
                    "快捷键监听已暂停，请重新启动 Winlane。",
                    "Shortcut monitoring is paused. Restart Winlane."
                )
                .into(),
            ));
            self.report_shortcut_status();
        }
    }

    fn report_shortcut_status(&self) {
        if let Some(settings) = self.settings_window() {
            if let Some(error) = self.ivars().hotkey_error.borrow().as_deref() {
                settings.report(error, true);
            } else {
                let config = self.ivars().config.borrow();
                settings.report(
                    &trf!(
                        "搜索 {} · 切换 {} 已启用。",
                        "Search {} · Switch {} enabled.",
                        config.shortcut.display(),
                        config.switch_shortcut.display()
                    ),
                    false,
                );
            }
        }
    }

    fn drain_shortcut_actions(&self) {
        let actions: Vec<_> = self
            .ivars()
            .shortcut_rx
            .borrow()
            .as_ref()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();
        for action in actions {
            self.shortcut_action(action);
        }
    }

    fn shortcut_action(&self, action: Action) {
        match action.kind {
            ActionKind::LaunchApp(index) => self.launch_app_shortcut(index),
            ActionKind::Search => {
                if self.ivars().mode.get().is_some() && self.ivars().session.get() == action.session
                {
                    self.display_search(action.session);
                } else {
                    self.show_mode(PanelMode::Search, action.session, 0);
                }
            }
            ActionKind::Switch { direction, fresh } => {
                if fresh {
                    self.show_mode(PanelMode::Switch, action.session, direction);
                } else if self.ivars().session.get() == action.session {
                    self.move_selection(isize::from(direction));
                }
            }
            ActionKind::Accept if self.ivars().session.get() == action.session => {
                if let Some(selection) = self.ivars().switch_selection.borrow_mut().as_mut() {
                    selection.release();
                }
                self.commit_switch_if_ready();
            }
            ActionKind::Alias(ch)
                if self.ivars().session.get() == action.session
                    && self.ivars().mode.get() == Some(PanelMode::Switch) =>
            {
                self.ivars().alias_input.borrow_mut().push(ch);
                self.select_alias();
                self.render();
            }
            ActionKind::AliasBackspace
                if self.ivars().session.get() == action.session
                    && self.ivars().mode.get() == Some(PanelMode::Switch) =>
            {
                self.ivars().alias_input.borrow_mut().pop();
                self.select_alias();
                self.render();
            }
            ActionKind::Cancel if self.ivars().session.get() == action.session => {
                self.end_session();
            }
            _ => {}
        }
    }

    fn autosave_settings(&self) {
        let Some(settings) = self.settings_window() else {
            return;
        };
        if self.ivars().saving_settings.replace(true) {
            return;
        }
        let result = settings
            .candidate()
            .and_then(|candidate| self.apply_config(candidate));
        self.ivars().saving_settings.set(false);
        if let Some(settings) = self.settings_window() {
            match result {
                Ok(()) => settings.report(tr!("已自动保存。", "Saved automatically."), false),
                Err(error) => settings.report(&trf!("未保存：{}", "Not saved: {}", error), true),
            }
        }
    }

    fn autosave_app_shortcuts(&self) {
        let Some(window) = self.app_shortcuts_window() else {
            return;
        };
        let result = window.candidate().and_then(|shortcuts| {
            let mut config = self.ivars().config.borrow().clone();
            config.app_shortcuts = shortcuts;
            self.apply_config(config)
        });
        match result {
            Ok(()) => window.report(
                tr!(
                    "有效的快捷键已自动保存；未选择应用的行暂不启用。",
                    "Valid shortcuts saved automatically. Rows without an app stay inactive."
                ),
                false,
            ),
            Err(error) => window.report(&trf!("未保存：{}", "Not saved: {}", error), true),
        }
    }

    fn apply_config(&self, candidate: Config) -> Result<(), String> {
        candidate.validate()?;
        let previous = self.ivars().config.borrow().clone();
        if candidate == previous {
            return Ok(());
        }
        let bindings_changed = candidate.shortcut != previous.shortcut
            || candidate.switch_shortcut != previous.switch_shortcut
            || candidate.app_bindings()? != previous.app_bindings()?;
        let registration = if bindings_changed {
            Some(ShortcutTap::new(
                self.mtm(),
                candidate.shortcut.binding()?,
                candidate.switch_shortcut.binding()?,
                candidate.app_bindings()?,
                self.ivars().wake.get().unwrap().handle(),
            )?)
        } else {
            None
        };
        settings::save(
            &candidate,
            self.ivars()
                .config_store
                .get_or_init(NSUserDefaults::standardUserDefaults),
        )?;
        if let Some((tap, receiver)) = registration {
            self.ivars().shortcut_tap.replace(Some(tap));
            self.ivars().shortcut_rx.replace(Some(receiver));
            self.ivars().hotkey_error.replace(None);
        }
        if candidate.appearance != previous.appearance {
            settings::apply_appearance(&candidate, self.mtm());
        }
        if let Some(settings) = self.settings_window() {
            settings.set_app_shortcuts(&candidate.app_shortcuts);
            settings.set_alias_rules(&candidate.alias_rules);
        }
        let language_changed =
            candidate.language != previous.language && settings::apply_language(candidate.language);
        self.ivars().config.replace(candidate);
        self.update_aliases(&self.ivars().windows.borrow());
        if language_changed {
            self.rebuild_localized_ui();
        }
        if bindings_changed {
            self.update_shortcut_labels();
        }
        self.filter();
        Ok(())
    }

    fn show_app_shortcuts(&self) {
        self.cancel_routing();
        self.end_session();
        self.ivars().launch_receiver.replace(None);
        let window = self.ensure_app_shortcuts_window();
        NSApplication::sharedApplication(self.mtm()).activate();
        window.show(
            &self.ivars().config.borrow().app_shortcuts,
            self,
            self.mtm(),
        );
    }

    fn launch_app_shortcut(&self, index: usize) {
        let Some(item) = self
            .ivars()
            .config
            .borrow()
            .app_shortcuts
            .get(index)
            .cloned()
        else {
            return;
        };
        self.launch_application(&item.application, LaunchOrigin::Shortcut);
    }

    fn launch_application(&self, application: &ApplicationTarget, origin: LaunchOrigin) {
        self.remember_frontmost_window();
        self.end_session();
        self.ivars().launch_receiver.replace(None);
        if let Some(window) = self.ivars().alias_rules.borrow().clone() {
            window.window.orderOut(None);
        }
        if let Some(settings) = self.settings_window() {
            settings.window.orderOut(None);
        }
        if let Some(settings) = self.app_shortcuts_window() {
            settings.window.orderOut(None);
        }
        match app_shortcuts::launch(application, self.ivars().wake.get().unwrap().handle()) {
            Ok(receiver) => {
                self.ivars()
                    .launch_receiver
                    .replace(Some(PendingLaunch { receiver, origin }));
            }
            Err(error) => self.report_launch_error(&error, origin),
        }
    }

    fn poll_app_launch(&self) {
        let result = self
            .ivars()
            .launch_receiver
            .borrow()
            .as_ref()
            .map(|pending| (pending.receiver.try_recv(), pending.origin));
        let (result, origin) = match result {
            Some((Ok(result), origin)) => (result, origin),
            Some((Err(TryRecvError::Disconnected), origin)) => (
                Err(tr!(
                    "应用启动未完成，请重试。",
                    "The app did not finish launching. Try again."
                )
                .into()),
                origin,
            ),
            _ => return,
        };
        self.ivars().launch_receiver.replace(None);
        match result {
            Ok(pid) => {
                if let Some(app) =
                    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
                {
                    app.unhide();
                    self.remember_application(pid);
                } else {
                    self.report_launch_error(
                        tr!(
                            "应用启动后已退出，请检查应用状态。",
                            "The app exited after launching. Check its status."
                        ),
                        origin,
                    )
                }
            }
            Err(error) => self.report_launch_error(&error, origin),
        }
    }

    fn report_launch_error(&self, error: &str, origin: LaunchOrigin) {
        match origin {
            LaunchOrigin::Shortcut => {
                self.show_app_shortcuts();
                if let Some(window) = self.app_shortcuts_window() {
                    window.report(error, true);
                }
            }
            LaunchOrigin::Search => {
                self.display_search(self.ivars().session.get());
                self.present_panels();
                self.report_switch_error(error);
            }
        }
    }

    fn update_shortcut_labels(&self) {
        let display = self.ivars().config.borrow().shortcut.display();
        self.ivars()
            .show_menu_item
            .borrow()
            .as_ref()
            .unwrap()
            .setTitle(&NSString::from_str(&trf!(
                "打开窗口搜索    {display}",
                "Open Window Search    {display}"
            )));
    }

    fn menu_item(&self, title: &str, action: Sel, key: &str) -> Retained<NSMenuItem> {
        // SAFETY: This retained delegate implements the selectors used by these menu items.
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(self.mtm()),
                &NSString::from_str(title),
                Some(action),
                &NSString::from_str(key),
            )
        };
        unsafe { item.setTarget(Some(self)) };
        item
    }

    fn add_window_actions(&self, menu: &NSMenu) {
        menu.addItem(&self.menu_item(
            tr!(
                "最小化 / 恢复所选窗口",
                "Minimize / Restore Selected Window"
            ),
            sel!(minimizeChosen:),
            "m",
        ));
        menu.addItem(&self.menu_item(
            tr!("隐藏所选应用", "Hide Selected App"),
            sel!(hideChosen:),
            "h",
        ));
        let copy = self.menu_item(
            tr!("复制窗口标题", "Copy Window Title"),
            sel!(copyTitle:),
            "c",
        );
        copy.setKeyEquivalentModifierMask(
            NSEventModifierFlags::Command | NSEventModifierFlags::Shift,
        );
        menu.addItem(&copy);
    }

    fn show(&self) {
        self.cancel_routing();
        let action = self
            .ivars()
            .shortcut_tap
            .borrow()
            .as_ref()
            .map(ShortcutTap::open_search);
        let session = action.map_or_else(
            || self.ivars().session.get().wrapping_add(1),
            |action| action.session,
        );
        self.show_mode(PanelMode::Search, session, 0);
    }

    fn open_switcher(&self) {
        let action = self
            .ivars()
            .shortcut_tap
            .borrow()
            .as_ref()
            .map(|tap| tap.enter_switch(NSEvent::modifierFlags_class().bits() as u64));
        if let Some(action) = action {
            self.shortcut_action(action);
        } else {
            self.show();
        }
    }

    fn show_mode(&self, mode: PanelMode, session: u64, direction: i8) {
        let already_visible = self.any_panel_visible();
        self.cancel_switch_timer();
        self.finish_search_input();
        if mode == PanelMode::Search {
            self.prepare_search_input();
        }
        self.ivars().launch_receiver.replace(None);
        if let Some(window) = self.ivars().alias_rules.borrow().clone() {
            window.window.orderOut(None);
        }
        if let Some(settings) = self.app_shortcuts_window() {
            settings.window.orderOut(None);
        }
        if let Some(settings) = self.settings_window() {
            settings.window.orderOut(None);
        }
        let preserve = self.selected_result();
        let preserve_window = self.selected_window().map(|window| window.id);
        let deferred = self.ivars().deferred_windows.borrow_mut().take();
        if let Some(windows) = deferred {
            self.install_windows(windows);
        }
        self.remember_frontmost_window();
        self.ivars().query.borrow_mut().clear();
        self.ivars().session.set(session);
        self.ivars().mode.set(Some(mode));
        self.ivars().alias_input.borrow_mut().clear();
        self.ivars()
            .switch_selection
            .replace((mode == PanelMode::Switch).then(|| SwitchSelection::new(direction)));
        self.ivars().switch_anchor.set(if direction == 0 {
            preserve_window
        } else {
            None
        });
        if mode == PanelMode::Switch {
            self.ivars().current_app_only.set(false);
        }
        self.filter_preserving(preserve);
        self.sync_displays();
        self.prepare_switch_selection();
        if !self.ivars().demo.get() {
            self.refresh();
        }
        if mode == PanelMode::Switch && !already_visible {
            self.schedule_switch_panel();
        } else {
            self.present_panels();
        }
    }

    fn cancel_switch_timer(&self) -> bool {
        if let Some(timer) = self.ivars().switch_timer.borrow_mut().take() {
            timer.invalidate();
            true
        } else {
            false
        }
    }

    fn schedule_switch_panel(&self) {
        self.cancel_switch_timer();
        let delay = self.ivars().config.borrow().switch_delay_ms;
        if delay == 0 {
            self.present_panels();
            return;
        }
        // SAFETY: One-shot main-thread timer; invalidated on cancellation, mode
        // change, or confirmation. Identity checks reject stale callbacks.
        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                f64::from(delay) / 1000.0,
                self,
                sel!(presentSwitch:),
                None,
                false,
            )
        };
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
        self.ivars().switch_timer.replace(Some(timer));
    }

    fn focus_search(&self) {
        if self.ivars().changing_input_source.replace(true) {
            return;
        }
        for ui in self
            .panels()
            .into_iter()
            .filter(|ui| ui.panel.isKeyWindow())
        {
            match self.ivars().mode.get() {
                Some(PanelMode::Search) => {
                    let new_editor = ui.input.currentEditor().is_none();
                    if new_editor {
                        ui.panel.makeFirstResponder(Some(&ui.input));
                    }
                    let editor = ui
                        .input
                        .currentEditor()
                        .and_then(|editor| editor.downcast::<NSTextView>().ok());
                    if let Some(editor) = editor
                        && !NSTextInputClient::hasMarkedText(&*editor)
                    {
                        let first_focus = !self.ivars().input_session.borrow().focused;
                        let source = if first_focus {
                            input_source::preferred(
                                self.ivars().config.borrow().input_method,
                                self.mtm(),
                            )
                        } else if new_editor {
                            self.ivars()
                                .input_session
                                .borrow()
                                .selected
                                .as_deref()
                                .and_then(|id| Source::by_id(id, self.mtm()))
                        } else {
                            None
                        };
                        if let Some(source) = source {
                            source.select(self.mtm());
                        }
                        self.ivars().input_session.borrow_mut().focused = true;
                        self.remember_search_input();
                    }
                }
                Some(PanelMode::Switch) => {
                    ui.panel.makeFirstResponder(None);
                }
                _ => {}
            }
        }
        self.ivars().changing_input_source.set(false);
    }

    fn prepare_search_input(&self) {
        let current = Source::current(self.mtm()).and_then(|source| source.id());
        self.ivars()
            .input_session
            .borrow_mut()
            .prepare(current, self.ivars().config.borrow().input_method);
    }

    fn remember_search_input(&self) {
        if self.ivars().mode.get() != Some(PanelMode::Search)
            || !self.any_panel_key()
            || !self.ivars().input_session.borrow().focused
        {
            return;
        }
        let current = Source::current(self.mtm()).and_then(|source| source.id());
        let mut session = self.ivars().input_session.borrow_mut();
        if session.observe(current)
            && let Some(id) = &session.selected
        {
            input_source::remember(id);
        }
    }

    fn finish_search_input(&self) {
        if !self.ivars().input_session.borrow().focused {
            self.ivars().input_session.borrow_mut().finish(None);
            return;
        }
        self.remember_search_input();
        // Do not overwrite a destination app's choice after focus has already moved away.
        let current = self
            .any_panel_key()
            .then(|| Source::current(self.mtm()))
            .flatten()
            .and_then(|source| source.id());
        let previous = self
            .ivars()
            .input_session
            .borrow_mut()
            .finish(current.as_deref());
        if let Some(previous) = previous.and_then(|id| Source::by_id(&id, self.mtm())) {
            self.ivars().changing_input_source.set(true);
            previous.select(self.mtm());
            self.ivars().changing_input_source.set(false);
        }
    }

    fn dismiss(&self) {
        self.cancel_routing();
        self.end_session();
        if let Some(previous) = NSRunningApplication::runningApplicationWithProcessIdentifier(
            self.ivars().previous_pid.get(),
        ) {
            self.activate_app(&previous);
        }
    }

    fn cancel_routing(&self) {
        if let Some(tap) = self.ivars().shortcut_tap.borrow().as_ref() {
            tap.cancel();
        }
        if let Some(receiver) = self.ivars().shortcut_rx.borrow().as_ref() {
            for _ in receiver.try_iter() {}
        }
    }

    fn end_session(&self) {
        self.cancel_switch_timer();
        self.finish_search_input();
        let state = self.ivars();
        if state.mode.replace(None).is_none() {
            return;
        }
        if let Some(tap) = state.shortcut_tap.borrow().as_ref() {
            tap.finish(state.session.get());
        }
        state.switch_selection.replace(None);
        state.check_panel_focus.set(false);
        for ui in self.panels() {
            ui.panel.orderOut(None);
        }
        let deferred = state.deferred_windows.borrow_mut().take();
        if let Some(windows) = deferred {
            self.install_windows(windows);
            self.filter();
        }
    }

    fn toggle_mode(&self, flags: u64) {
        if self.ivars().mode.get() == Some(PanelMode::Switch) {
            self.switch_to_search();
        } else {
            let action = self
                .ivars()
                .shortcut_tap
                .borrow()
                .as_ref()
                .map(|tap| tap.enter_switch(flags));
            if let Some(action) = action {
                self.shortcut_action(action);
            }
        }
    }

    fn switch_to_search(&self) {
        let state = self.ivars();
        let action = state
            .shortcut_tap
            .borrow()
            .as_ref()
            .map(ShortcutTap::open_search);
        self.display_search(action.map_or(state.session.get(), |action| action.session));
    }

    fn display_search(&self, session: u64) {
        let was_delayed = self.cancel_switch_timer();
        if self.ivars().mode.get() != Some(PanelMode::Search) {
            self.prepare_search_input();
        }
        let state = self.ivars();
        let selected_id = self.selected_result();
        state.session.set(session);
        state.mode.set(Some(PanelMode::Search));
        state.alias_input.borrow_mut().clear();
        state.switch_selection.replace(None);
        let deferred = state.deferred_windows.borrow_mut().take();
        if let Some(windows) = deferred {
            self.install_windows(windows);
        }
        self.filter_preserving(selected_id);
        if was_delayed {
            self.present_panels();
        }
        self.focus_search();
    }

    fn prepare_switch_selection(&self) {
        let state = self.ivars();
        if state.mode.get() != Some(PanelMode::Switch) {
            return;
        }
        let matches = state.matches.borrow();
        let windows = state.windows.borrow();
        let anchor = if let Some(id) = state.switch_anchor.get() {
            matches.iter().position(|&index| windows[index].id == id)
        } else {
            state
                .previous_window
                .get()
                .and_then(|id| matches.iter().position(|&index| windows[index].id == id))
                .or_else(|| {
                    matches
                        .iter()
                        .position(|&index| windows[index].pid == state.previous_pid.get())
                })
        };
        if let Some(selection) = state.switch_selection.borrow_mut().as_mut() {
            selection.install(matches.len(), anchor);
            if let Some(index) = selection.selected() {
                state.selected.set(index);
            }
        }
        drop(windows);
        drop(matches);
        self.select_alias();
        self.render();
    }

    fn alias_position(&self) -> Option<usize> {
        self.alias_match().position()
    }

    fn alias_match(&self) -> AliasMatch {
        let state = self.ivars();
        let windows = state.windows.borrow();
        let ordered_windows: Vec<_> = state
            .matches
            .borrow()
            .iter()
            .map(|&index| windows[index].id)
            .collect();
        state
            .aliases
            .borrow()
            .match_windows(state.alias_input.borrow().text(), &ordered_windows)
    }

    fn select_alias(&self) {
        if let Some(index) = self.alias_position() {
            if let Some(selection) = self.ivars().switch_selection.borrow_mut().as_mut() {
                selection.select(index);
            }
            self.ivars().selected.set(index);
        }
    }

    fn commit_switch_if_ready(&self) {
        let state = self.ivars();
        if state.mode.get() != Some(PanelMode::Switch) {
            return;
        }
        if !state.alias_input.borrow().text().is_empty() && self.alias_position().is_none() {
            let released = state
                .switch_selection
                .borrow()
                .as_ref()
                .is_some_and(SwitchSelection::released);
            if released && !state.loading.get() {
                self.end_session();
            }
            return;
        }
        let commit = state
            .switch_selection
            .borrow_mut()
            .as_mut()
            .and_then(SwitchSelection::take_commit);
        if let Some(index) = commit {
            state.selected.set(index);
            self.activate_selected();
        } else if !state.loading.get()
            && state
                .switch_selection
                .borrow()
                .as_ref()
                .is_some_and(SwitchSelection::released)
        {
            self.end_session();
        }
    }

    fn refresh(&self) {
        let state = self.ivars();
        if !accessibility::is_trusted() {
            state.windows.borrow_mut().clear();
            state.loading.set(false);
            self.filter();
            return;
        }
        if state.receiver.borrow().is_some() {
            self.render();
            return;
        }
        let apps = NSWorkspace::sharedWorkspace()
            .runningApplications()
            .iter()
            .filter(|app| {
                app.processIdentifier() != std::process::id() as i32
                    && !app.isTerminated()
                    && app.activationPolicy() == NSApplicationActivationPolicy::Regular
            })
            .map(|app| {
                let id = app
                    .bundleIdentifier()
                    .map(|id| id.to_string())
                    .or_else(|| app.bundleURL()?.path().map(|path| path.to_string()));
                let cached = state
                    .identities
                    .borrow()
                    .get(&app.processIdentifier())
                    .is_some_and(|app| Some(app.id.as_str()) == id.as_deref());
                if !cached {
                    state.icons.borrow_mut().remove(&app.processIdentifier());
                    state
                        .identities
                        .borrow_mut()
                        .remove(&app.processIdentifier());
                    if let Some(identity) = app_identity(&app) {
                        state
                            .identities
                            .borrow_mut()
                            .insert(app.processIdentifier(), identity);
                    }
                }
                (
                    app.processIdentifier(),
                    app.localizedName()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "Application".into()),
                )
            })
            .collect::<Vec<_>>();
        let live: HashSet<_> = apps.iter().map(|(pid, _)| *pid).collect();
        state
            .identities
            .borrow_mut()
            .retain(|pid, _| live.contains(pid));
        state.icons.borrow_mut().retain(|pid, _| live.contains(pid));
        state.loading.set(true);
        let (tx, rx) = mpsc::channel();
        state.receiver.replace(Some(rx));
        let wake = state.wake.get().unwrap().handle();
        std::thread::spawn(move || {
            let _ = tx.send(accessibility::list_windows(&apps));
            wake.signal();
        });
        self.render();
    }

    fn install_windows(&self, windows: Vec<WindowInfo>) {
        self.update_aliases(&windows);
        self.ivars().windows.replace(windows);
    }

    fn update_aliases(&self, windows: &[WindowInfo]) {
        let mut aliases = self.ivars().automatic_aliases.borrow_mut();
        if aliases.ensure_windows(windows, &self.ivars().identities.borrow())
            && self.ivars().aliases_writable.get()
        {
            settings::save_aliases(&aliases);
        }
        let effective = aliases.with_rules(
            windows,
            &self.ivars().identities.borrow(),
            &self.ivars().config.borrow().alias_rules,
        );
        self.ivars().aliases.replace(effective);
    }

    fn ensure_app_catalog(&self) {
        let state = self.ivars();
        if state.demo.get()
            || state.mode.get() != Some(PanelMode::Search)
            || state.current_app_only.get()
            || state.query.borrow().trim().is_empty()
            || state.catalog_receiver.borrow().is_some()
            || state
                .catalog_checked
                .get()
                .is_some_and(|at| at.elapsed() < APP_CATALOG_TTL)
        {
            return;
        }
        let (tx, rx) = mpsc::channel();
        state.catalog_receiver.replace(Some(rx));
        let wake = state.wake.get().unwrap().handle();
        std::thread::spawn(move || {
            let _ = tx.send(installed_apps::discover());
            wake.signal();
        });
    }

    fn poll_app_catalog(&self) {
        let state = self.ivars();
        let result = state
            .catalog_receiver
            .borrow()
            .as_ref()
            .map(|rx| rx.try_recv());
        match result {
            Some(Ok(apps)) => {
                let selected = self.selected_result();
                state.catalog_receiver.replace(None);
                state.catalog_checked.set(Some(Instant::now()));
                state.installed_apps.replace(apps);
                if state.mode.get() == Some(PanelMode::Search) {
                    self.filter_preserving(selected);
                }
            }
            Some(Err(TryRecvError::Disconnected)) => {
                state.catalog_receiver.replace(None);
            }
            _ => {}
        }
    }

    fn filter(&self) {
        self.filter_preserving(None);
    }

    fn filter_preserving(&self, selected_id: Option<SelectedResult>) {
        self.ensure_app_catalog();
        let query = self.ivars().query.borrow().clone();
        let preferred = self
            .ivars()
            .preferences
            .borrow()
            .get(&query.trim().to_lowercase())
            .copied();
        let windows = self.ivars().windows.borrow();
        let scope_pid = if self.ivars().current_app_only.get() {
            Some(if self.ivars().demo.get() {
                -1
            } else {
                self.ivars().previous_pid.get()
            })
        } else {
            None
        };
        let aliases = self.ivars().aliases.borrow();
        let is_alias = aliases.is_alias(&query);
        let matched = visible_matches(
            &windows,
            if is_alias { "" } else { &query },
            preferred,
            &self.ivars().config.borrow(),
            scope_pid,
            &self.ivars().recency.borrow(),
            self.ivars().previous_pid.get(),
        );
        let matched = aliases
            .filter_order(&query, &matched, &windows)
            .unwrap_or(matched);
        let apps = self.ivars().installed_apps.borrow();
        let launch_matches = if self.ivars().mode.get() == Some(PanelMode::Search)
            && !self.ivars().demo.get()
            && scope_pid.is_none()
            && aliases.resolve_window(&query).is_none()
            && (!aliases.is_alias(&query) || aliases.resolve(&query).is_some())
        {
            let identities = self.ivars().identities.borrow();
            let occupied: HashSet<_> = windows
                .iter()
                .filter_map(|window| identities.get(&window.pid))
                .map(|app| app.id.clone())
                .collect();
            matching_apps(
                &apps,
                &query,
                &occupied,
                &self.ivars().config.borrow().excluded_apps,
                aliases.resolve(&query),
            )
        } else {
            Vec::new()
        };
        drop(aliases);
        let selected = match selected_id {
            Some(SelectedResult::Window(id)) => {
                matched.iter().position(|&index| windows[index].id == id)
            }
            Some(SelectedResult::Application(id)) => launch_matches
                .iter()
                .position(|&index| apps[index].target.bundle_id == id)
                .map(|index| matched.len() + index)
                .or_else(|| {
                    let identities = self.ivars().identities.borrow();
                    matched.iter().position(|&index| {
                        identities
                            .get(&windows[index].pid)
                            .is_some_and(|app| app.id == id)
                    })
                }),
            None => None,
        }
        .unwrap_or(0);
        let visible_paths: HashSet<_> = launch_matches
            .iter()
            .map(|&index| apps[index].target.path.as_str())
            .collect();
        self.ivars()
            .application_icons
            .borrow_mut()
            .retain(|path, _| visible_paths.contains(path.as_str()));
        drop(apps);
        drop(windows);
        self.ivars().matches.replace(matched);
        self.ivars().launch_matches.replace(launch_matches);
        self.ivars().selected.set(selected);
        self.render();
    }

    fn move_selection(&self, direction: isize) {
        if self.ivars().mode.get() == Some(PanelMode::Switch) {
            self.ivars().alias_input.borrow_mut().clear();
            if let Some(selection) = self.ivars().switch_selection.borrow_mut().as_mut() {
                selection.step(direction.signum() as i8);
                if let Some(index) = selection.selected() {
                    self.ivars().selected.set(index);
                }
            }
            self.render();
            return;
        }
        let count = self.match_count();
        if count == 0 {
            return;
        }
        self.ivars().selected.set(
            (self.ivars().selected.get() as isize + direction).rem_euclid(count as isize) as usize,
        );
        self.render();
    }

    fn render(&self) {
        if self.ivars().mode.get().is_none() {
            return;
        }
        self.ivars().syncing_controls.set(true);
        for ui in self.panels() {
            self.render_panel(&ui);
        }
        self.ivars().syncing_controls.set(false);
    }

    fn render_panel(&self, ui: &PanelUi) {
        let opacity = f64::from(self.ivars().config.borrow().background_opacity) / 100.0;
        if ui.backdrop.alphaValue() != opacity {
            ui.backdrop.setAlphaValue(opacity);
        }
        let query = self.ivars().query.borrow();
        if ui.input.stringValue().to_string() != *query {
            ui.input.setStringValue(&NSString::from_str(&query));
        }
        drop(query);
        let state = self.ivars();
        let list = &ui.list;
        let windows = state.windows.borrow();
        let matched = state.matches.borrow();
        let launch_matches = state.launch_matches.borrow();
        let apps = state.installed_apps.borrow();
        let count = matched.len() + launch_matches.len();
        let trusted = accessibility::is_trusted();
        let demo = state.demo.get();
        let switching = state.mode.get() == Some(PanelMode::Switch);
        let root = ui.panel.contentView().unwrap();
        let previous_height = root.bounds().size.height;
        let mut frame = ui.panel.frame();
        let chrome_height = frame.size.height - previous_height;
        let visible = ui.panel.screen().map(|screen| screen.visibleFrame());
        let mut height = panel_height(count, switching, !trusted || demo);
        if let Some(visible) = visible {
            height = height.min((visible.size.height - chrome_height).max(1.0));
        }
        if (height - previous_height).abs() > 0.5 {
            // Keep the search field stationary as results shrink; only shift the
            // top edge when growing would otherwise extend below the display.
            frame.origin.y += previous_height - height;
            frame.size.height = height + chrome_height;
            if let Some(visible) = visible {
                frame.origin.y = frame.origin.y.clamp(
                    visible.origin.y,
                    (visible.origin.y + visible.size.height - frame.size.height)
                        .max(visible.origin.y),
                );
            }
            ui.panel.setFrame_display(frame, false);
        }
        let header: [(&NSView, f64); 2] = [
            (&ui.input, height - 52.0),
            (
                &ui.shortcut_label,
                height - if switching { 26.0 } else { 44.0 },
            ),
        ];
        for (view, y) in header {
            let origin = NSPoint::new(view.frame().origin.x, y);
            if view.frame().origin != origin {
                view.setFrameOrigin(origin);
            }
        }
        let alias_input = state.alias_input.borrow();
        let alias_query = alias_input.text();
        let aliases = state.aliases.borrow();
        let alias_match = self.alias_match();
        let unmatched_alias =
            switching && !alias_query.is_empty() && alias_match.position().is_none();
        ui.input.setHidden(switching);
        ui.mode_label.setHidden(!switching);
        let config = state.config.borrow();
        ui.mode_label
            .setStringValue(&NSString::from_str(&if alias_query.is_empty() {
                trf!(
                    "切换模式 · {} 选择 · 松开 {} 或 ↵ 确认",
                    "Switch · {} to select · Release {} or ↵ to confirm",
                    config.switch_shortcut.display(),
                    config.switch_shortcut.release_label()
                )
            } else {
                format!(
                    "Alias  {alias_query}  ·  {}",
                    if unmatched_alias {
                        if alias_match == AliasMatch::Ambiguous {
                            tr!(
                                "多个应用匹配 · 继续输入第二个字母",
                                "Multiple matches · Type a second letter"
                            )
                        } else {
                            tr!(
                                "没有匹配窗口 · Backspace 修改",
                                "No matching window · Backspace to edit"
                            )
                        }
                    } else {
                        tr!(
                            "松开修饰键确认 · Backspace 修改",
                            "Release modifier to confirm · Backspace to edit"
                        )
                    }
                )
            }));
        ui.shortcut_label
            .setStringValue(&NSString::from_str(&if switching {
                config.switch_shortcut.display()
            } else {
                config.shortcut.display()
            }));
        drop(config);
        ui.help.setHidden(trusted || demo);
        ui.refresh_button.setHidden(trusted || demo);
        ui.demo_button.setHidden(trusted && !demo);
        ui.demo_button.setTitle(&NSString::from_str(if demo {
            tr!("返回真实窗口", "Show Real Windows")
        } else {
            tr!("查看演示", "View Demo")
        }));
        let list_bottom = if !trusted || demo { 64.0 } else { LIST_BOTTOM };
        let mode_frame = rect(16.0, list_bottom, WIDTH - 32.0, 20.0);
        if ui.mode_label.frame() != mode_frame {
            ui.mode_label.setFrame(mode_frame);
        }
        let list_bottom = list_bottom + if switching { ROW_HEIGHT } else { 0.0 };
        let list_top = height - if switching { 36.0 } else { HEIGHT - LIST_TOP };
        let list_height = list_top - list_bottom;
        let scroll_frame = rect(10.0, list_bottom, LIST_WIDTH, list_height);
        if ui.scroll.frame() != scroll_frame {
            ui.scroll.setFrame(scroll_frame);
        }
        let list_size = NSSize::new(LIST_WIDTH, (count as f64 * ROW_HEIGHT).max(list_height));
        if list.frame().size != list_size {
            list.setFrameSize(list_size);
        }
        let mut rows = ui.rows.borrow_mut();
        for row in rows.iter_mut().skip(count) {
            if row.attached {
                row.button.removeFromSuperview();
                row.attached = false;
            }
        }
        rows.truncate(count.max(windows.len().min(128)));
        for label in ui.empty_labels.borrow_mut().drain(..) {
            label.removeFromSuperview();
        }
        if count == 0 {
            let (title, detail) = if state.loading.get() {
                (
                    tr!("正在读取窗口…", "Reading windows…"),
                    tr!(
                        "窗口枚举在后台进行，搜索界面仍可输入。",
                        "Loading windows in the background. You can keep typing."
                    ),
                )
            } else if !switching
                && !state.query.borrow().trim().is_empty()
                && state.catalog_receiver.borrow().is_some()
            {
                (
                    tr!("正在查找应用…", "Finding apps…"),
                    tr!("可以继续输入关键词。", "You can keep typing."),
                )
            } else if !trusted && !demo {
                (
                    tr!(
                        "先允许 Winlane 控制窗口",
                        "Allow Winlane to control windows"
                    ),
                    tr!(
                        "在系统设置 → 隐私与安全性的应用控制权限中启用 Winlane。\nmacOS 27：Device Control and Data Access；旧版：辅助功能。\n用于读取与切换窗口。授权后重新呼出搜索面板即可。",
                        "Enable Winlane in System Settings → Privacy & Security.\nmacOS 27: Device Control and Data Access; earlier: Accessibility.\nThis allows reading and switching windows. Reopen the panel after granting access."
                    ),
                )
            } else {
                (
                    if !switching && !state.query.borrow().trim().is_empty() {
                        tr!("没有匹配的窗口或应用", "No matching windows or apps")
                    } else {
                        tr!("没有匹配的窗口", "No matching windows")
                    },
                    tr!(
                        "试试其他关键词，或检查当前应用筛选和排除设置。",
                        "Try other keywords, or check the current-app filter and excluded apps."
                    ),
                )
            };
            let message_top = ((list_height - 142.0) / 2.0).max(12.0);
            let heading = label(
                title,
                21.0,
                rect(40.0, message_top, 540.0, 38.0),
                self.mtm(),
            );
            let detail = label(
                detail,
                14.0,
                rect(40.0, message_top + 42.0, 550.0, 96.0),
                self.mtm(),
            );
            detail.setTextColor(Some(&NSColor::secondaryLabelColor()));
            detail.setMaximumNumberOfLines(4);
            list.addSubview(&heading);
            list.addSubview(&detail);
            ui.empty_labels.replace(vec![heading, detail]);
        }
        for position in 0..count {
            let content = if let Some(&index) = matched.get(position) {
                let item = &windows[index];
                RowContent::Window(item.clone(), aliases.for_window(item.id).map(str::to_owned))
            } else {
                RowContent::Application(
                    apps[launch_matches[position - matched.len()]]
                        .target
                        .clone(),
                )
            };
            let selected = position == state.selected.get() && !unmatched_alias;
            if rows.len() <= position {
                rows.push(self.create_row(position));
            }
            let row = &mut rows[position];
            if row.content.as_ref() != Some(&content) {
                let tooltip = match &content {
                    RowContent::Window(item, alias) => {
                        let title = if item.title.trim().is_empty() {
                            &item.app
                        } else {
                            &item.title
                        };
                        let tooltip = format!(
                            "{} — {}{}",
                            item.app,
                            title,
                            alias
                                .as_deref()
                                .map_or(String::new(), |alias| format!(" · alias {alias}"))
                        );
                        let title = if item.minimized {
                            trf!("{title} · 已最小化", "{title} · Minimized")
                        } else {
                            title.clone()
                        };
                        set_label(&row.title, &title);
                        set_label(&row.app, &item.app);
                        set_label(&row.alias, alias.as_deref().unwrap_or(""));
                        row.icon.setImage(self.icon(item.pid).as_deref());
                        tooltip
                    }
                    RowContent::Application(app) => {
                        set_label(&row.title, tr!("启动应用", "Launch app"));
                        set_label(&row.app, &app.name);
                        set_label(&row.alias, "↗");
                        row.icon.setImage(Some(&self.application_icon(app)));
                        trf!("启动 {} — {}", "Launch {} — {}", app.name, app.path)
                    }
                };
                row.button.setToolTip(Some(&NSString::from_str(&tooltip)));
                row.button
                    .setAccessibilityLabel(Some(&NSString::from_str(&tooltip)));
                row.content = Some(content);
                row.selected = None;
            }
            row.select(selected);
            if !row.attached {
                list.addSubview(&row.button);
                row.attached = true;
            }
            if selected {
                row.button.scrollRectToVisible(row.button.bounds());
            }
        }

        let status = if demo {
            trf!(
                "演示模式 · {} 个示例 · ↑↓ 选择  ↵ 预览选择  Esc 关闭",
                "Demo · {} examples · ↑↓ select · ↵ preview · Esc close",
                matched.len()
            )
        } else if let Some(error) = state.hotkey_error.borrow().as_ref() {
            error.clone()
        } else if let Some(error) = state.alias_error.borrow().as_ref() {
            error.clone()
        } else if !trusted {
            tr!(
                "需要辅助功能权限；也可以先查看演示。",
                "Accessibility access required. You can also try the demo."
            )
            .into()
        } else if state.loading.get() {
            tr!("正在刷新窗口…", "Refreshing windows…").into()
        } else if switching {
            trf!(
                "{} 项 · 字母定位 · ↑↓ 选择 · Space 搜索 · Esc 取消",
                "{} items · Type alias · ↑↓ select · Space search · Esc cancel",
                matched.len()
            )
        } else if !launch_matches.is_empty() {
            trf!(
                "{} 个窗口 · {} 个应用 · ↵ 切换 / 启动 · Esc 取消",
                "{} windows · {} apps · ↵ switch / launch · Esc cancel",
                matched.len(),
                launch_matches.len()
            )
        } else {
            trf!(
                "{} 项 · 输入 alias 或标题 · ↵ 切换 · Space 切换模式",
                "{} items · Type alias or title · ↵ switch · Space changes mode",
                matched.len()
            )
        };
        set_label(&ui.footer, &status);
    }

    fn create_row(&self, position: usize) -> RowUi {
        let mtm = self.mtm();
        let frame = rect(
            2.0,
            position as f64 * ROW_HEIGHT + 1.0,
            LIST_WIDTH - 4.0,
            ROW_HEIGHT - 2.0,
        );
        // SAFETY: WindowRowButton inherits NSButton's designated frame initializer.
        let button: Retained<WindowRowButton> = unsafe {
            msg_send![super(WindowRowButton::alloc(mtm).set_ivars(RowAppearance::default())), initWithFrame: frame]
        };
        button.setTitle(ns_string!(""));
        button.setBordered(false);
        button.setTag(position as isize);
        // SAFETY: The delegate outlives its rows and pickWindow: takes a button sender.
        unsafe {
            button.setTarget(Some(self));
            button.setAction(Some(sel!(pickWindow:)));
        }
        let title = label("", 13.0, rect(222.0, 3.0, LIST_WIDTH - 236.0, 20.0), mtm);
        let app = label("", 13.0, rect(42.0, 3.0, 142.0, 20.0), mtm);
        app.setFont(Some(&NSFont::systemFontOfSize_weight(12.0, unsafe {
            NSFontWeightMedium
        })));
        app.setAlignment(NSTextAlignment::Right);
        for field in [&title, &app] {
            field.setMaximumNumberOfLines(1);
            field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
            button.addSubview(field);
        }
        let alias = label("", 10.0, rect(8.0, 4.0, 28.0, 18.0), mtm);
        alias.setAlignment(NSTextAlignment::Center);
        alias.setFont(Some(&NSFont::monospacedSystemFontOfSize_weight(
            10.0,
            unsafe { NSFontWeightSemibold },
        )));
        button.addSubview(&alias);
        let icon =
            NSImageView::initWithFrame(NSImageView::alloc(mtm), rect(192.0, 3.0, 20.0, 20.0));
        icon.setImageScaling(NSImageScaling::ScaleProportionallyDown);
        button.addSubview(&icon);
        RowUi {
            button,
            title,
            app,
            alias,
            icon,
            content: None,
            selected: None,
            attached: false,
        }
    }

    fn icon(&self, pid: i32) -> Option<Retained<NSImage>> {
        self.ivars()
            .icons
            .borrow_mut()
            .entry(pid)
            .or_insert_with(|| {
                NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
                    .and_then(|app| app.icon())
                    .or_else(|| {
                        NSImage::imageWithSystemSymbolName_accessibilityDescription(
                            ns_string!("macwindow"),
                            Some(&NSString::from_str(tr!("窗口", "Window"))),
                        )
                    })
            })
            .clone()
    }

    fn activate_app(&self, app: &NSRunningApplication) -> bool {
        let current = NSRunningApplication::currentApplication();
        NSApplication::sharedApplication(self.mtm()).yieldActivationToApplication(app);
        app.activateFromApplication_options(&current, NSApplicationActivationOptions::empty())
    }

    fn application_icon(&self, app: &ApplicationTarget) -> Retained<NSImage> {
        self.ivars()
            .application_icons
            .borrow_mut()
            .entry(app.path.clone())
            .or_insert_with(|| {
                let source =
                    NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(&app.path));
                let image = NSImage::initWithSize(NSImage::alloc(), NSSize::new(20.0, 20.0));
                #[allow(deprecated)]
                {
                    image.lockFocus();
                    source.drawInRect(rect(0.0, 0.0, 20.0, 20.0));
                    image.unlockFocus();
                }
                image
            })
            .clone()
    }

    fn track_frontmost(&self) {
        if self.ivars().demo.get() {
            return;
        }
        let Some(app) = NSWorkspace::sharedWorkspace().frontmostApplication() else {
            return;
        };
        let pid = app.processIdentifier();
        if pid == std::process::id() as i32
            || app.activationPolicy() != NSApplicationActivationPolicy::Regular
        {
            return;
        }
        self.remember_application(pid);
        if self
            .ivars()
            .focus_observer
            .borrow()
            .as_ref()
            .is_some_and(|observer| observer.pid == pid)
        {
            return;
        }
        let weak = Weak::new(self);
        let observer = crate::focus_observer::FocusObserver::new(pid, self.mtm(), move |id| {
            if let Some(delegate) = weak.load()
                && !delegate.ivars().demo.get()
                && NSWorkspace::sharedWorkspace()
                    .frontmostApplication()
                    .is_some_and(|app| app.processIdentifier() == pid)
            {
                delegate.remember_window(id);
            }
        });
        self.ivars().focus_observer.replace(observer);
    }

    fn remember_frontmost_window(&self) {
        if self.ivars().demo.get() || self.any_panel_key() {
            return;
        }
        if let Some(front) = NSWorkspace::sharedWorkspace().frontmostApplication()
            && front.processIdentifier() != std::process::id() as i32
        {
            let pid = front.processIdentifier();
            self.ivars().previous_pid.set(pid);
            self.ivars()
                .previous_window
                .set(self.remember_application(pid));
        }
    }

    fn remember_application(&self, pid: i32) -> Option<u64> {
        let id = accessibility::focused_window(pid).or_else(|| {
            let windows = self.ivars().windows.borrow();
            let recent = self.ivars().recency.borrow();
            windows
                .iter()
                .filter(|window| window.pid == pid)
                .min_by_key(|window| {
                    (
                        window.minimized,
                        recent
                            .iter()
                            .position(|id| *id == window.id)
                            .unwrap_or(usize::MAX),
                    )
                })
                .map(|window| window.id)
        })?;
        self.remember_window(id);
        Some(id)
    }

    fn remember_window(&self, id: u64) {
        let mut recent = self.ivars().recency.borrow_mut();
        recent.retain(|previous| *previous != id);
        recent.insert(0, id);
        recent.truncate(128);
    }

    fn activate_selected(&self) {
        if let Some(tap) = self.ivars().shortcut_tap.borrow().as_ref() {
            tap.finish(self.ivars().session.get());
        }
        self.ivars().switch_selection.replace(None);
        if let Some(application) = self.selected_application() {
            self.launch_application(&application, LaunchOrigin::Search);
            return;
        }
        let Some(window) = self.selected_window() else {
            self.selection_failed(tr!(
                "没有匹配的窗口，请重新搜索。",
                "No matching window. Search again."
            ));
            return;
        };
        if self.ivars().demo.get() {
            self.selection_failed(&trf!(
                "演示选择：{} · {}（未切换真实窗口）",
                "Demo selection: {} · {} (real windows unchanged)",
                window.app,
                window.title
            ));
            return;
        }
        let Some(target_app) =
            NSRunningApplication::runningApplicationWithProcessIdentifier(window.pid)
        else {
            self.selection_failed(tr!(
                "应用已退出，请按 ⌘R 更新窗口列表。",
                "The app has quit. Press ⌘R to refresh windows."
            ));
            return;
        };
        target_app.unhide();
        if let Err(error) = accessibility::raise_window(window.pid, window.id) {
            self.selection_failed(&error);
            return;
        }
        if !self.activate_app(&target_app) {
            self.selection_failed(tr!(
                "系统未接受切换请求，请重试或检查辅助功能权限。",
                "macOS did not accept the switch. Try again or check Accessibility access."
            ));
            return;
        }
        self.remember_window(window.id);
        let query = self.ivars().query.borrow().trim().to_lowercase();
        if !query.is_empty() {
            let mut preferences = self.ivars().preferences.borrow_mut();
            if preferences.len() > 128 {
                preferences.clear();
            }
            preferences.insert(query, window.id);
        }
        self.end_session();
    }

    fn selection_failed(&self, text: &str) {
        let session = self.ivars().session.get();
        if let Some(tap) = self.ivars().shortcut_tap.borrow().as_ref() {
            tap.resume_search(session);
        }
        self.display_search(session);
        self.report_switch_error(text);
    }

    fn report_switch_error(&self, text: &str) {
        for ui in self.panels() {
            ui.footer.setStringValue(&NSString::from_str(text));
            ui.footer.setToolTip(Some(&NSString::from_str(text)));
        }
    }

    fn minimize_selected(&self) {
        if !self.any_panel_visible() {
            return;
        }
        if self.ivars().mode.get() == Some(PanelMode::Switch) {
            self.switch_to_search();
        }
        let Some(window) = self.selected_window() else {
            return;
        };
        let minimized = !window.minimized;
        if !self.ivars().demo.get()
            && let Err(error) = accessibility::set_minimized(window.pid, window.id, minimized)
        {
            self.report_switch_error(&error);
            return;
        }
        self.ivars().receiver.replace(None);
        self.ivars().loading.set(false);
        if let Some(item) = self
            .ivars()
            .windows
            .borrow_mut()
            .iter_mut()
            .find(|item| item.id == window.id)
        {
            item.minimized = minimized;
        }
        self.filter_preserving(Some(SelectedResult::Window(window.id)));
        self.report_switch_error(if self.ivars().demo.get() {
            tr!(
                "演示：已更新示例窗口的最小化状态，未操作真实窗口。",
                "Demo: minimized state updated; real windows unchanged."
            )
        } else if minimized {
            tr!("已最小化窗口。", "Window minimized.")
        } else {
            tr!("已恢复窗口。", "Window restored.")
        });
    }

    fn hide_selected(&self) {
        if !self.any_panel_visible() {
            return;
        }
        let Some(window) = self.selected_window() else {
            return;
        };
        if self.ivars().demo.get() {
            self.report_switch_error(&trf!(
                "演示：隐藏 {}（未操作真实应用）。",
                "Demo: hide {} (real apps unchanged).",
                window.app
            ));
            return;
        }
        match NSRunningApplication::runningApplicationWithProcessIdentifier(window.pid) {
            Some(app) if app.hide() => self.report_switch_error(tr!(
                "已隐藏应用。选择它的窗口后按回车可重新打开。",
                "App hidden. Select its window and press Enter to show it again."
            )),
            _ => self.report_switch_error(tr!(
                "无法隐藏应用，它可能已经退出。",
                "Could not hide the app. It may have quit."
            )),
        }
    }

    fn selected_window(&self) -> Option<WindowInfo> {
        let state = self.ivars();
        state
            .matches
            .borrow()
            .get(state.selected.get())
            .and_then(|&index| state.windows.borrow().get(index).cloned())
    }

    fn selected_application(&self) -> Option<ApplicationTarget> {
        let state = self.ivars();
        let index = state
            .selected
            .get()
            .checked_sub(state.matches.borrow().len())?;
        state.launch_matches.borrow().get(index).and_then(|&index| {
            state
                .installed_apps
                .borrow()
                .get(index)
                .map(|app| app.target.clone())
        })
    }

    fn selected_result(&self) -> Option<SelectedResult> {
        self.selected_window()
            .map(|window| SelectedResult::Window(window.id))
            .or_else(|| {
                self.selected_application()
                    .map(|app| SelectedResult::Application(app.bundle_id))
            })
    }

    fn match_count(&self) -> usize {
        self.ivars().matches.borrow().len() + self.ivars().launch_matches.borrow().len()
    }
}

fn display_rect(frame: NSRect) -> Rect {
    Rect {
        x: frame.origin.x,
        y: frame.origin.y,
        width: frame.size.width,
        height: frame.size.height,
    }
}

fn app_identity(app: &NSRunningApplication) -> Option<AppIdentity> {
    let url = app.bundleURL()?;
    let id = app
        .bundleIdentifier()
        .map(|id| id.to_string())
        .or_else(|| url.path().map(|path| path.to_string()))?;
    let info = NSBundle::bundleWithURL(&url).and_then(|bundle| bundle.infoDictionary());
    let metadata_name = ["CFBundleDisplayName", "CFBundleName", "CFBundleExecutable"]
        .iter()
        .filter_map(|key| {
            info.as_ref()?
                .objectForKey(&NSString::from_str(key))?
                .downcast::<NSString>()
                .ok()
                .map(|name| name.to_string())
        })
        .find(|name| name.chars().any(|ch| ch.is_ascii_alphabetic()));
    let english_name = metadata_name
        .or_else(|| {
            url.lastPathComponent()
                .map(|name| name.to_string().trim_end_matches(".app").to_owned())
        })
        .unwrap_or_default();
    Some(AppIdentity { id, english_name })
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}
fn panel_height(count: usize, switching: bool, extra_controls: bool) -> f64 {
    let header = if switching { 36.0 } else { HEIGHT - LIST_TOP };
    let footer =
        if extra_controls { 64.0 } else { LIST_BOTTOM } + if switching { ROW_HEIGHT } else { 0.0 };
    let content = if count == 0 {
        184.0
    } else {
        count as f64 * ROW_HEIGHT + 8.0
    };
    (header + footer + content).clamp(280.0, HEIGHT)
}
pub(crate) fn panel_backdrop(frame: NSRect, mtm: MainThreadMarker) -> Retained<NSVisualEffectView> {
    let backdrop = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), frame);
    backdrop.setMaterial(NSVisualEffectMaterial::Popover);
    backdrop.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    backdrop.setState(NSVisualEffectState::Active);
    let resize =
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable;
    backdrop.setAutoresizingMask(resize);
    // SAFETY: PanelSurface inherits NSView's frame initializer.
    let surface: Retained<PanelSurface> =
        unsafe { msg_send![PanelSurface::alloc(mtm), initWithFrame: backdrop.bounds()] };
    surface.setAutoresizingMask(resize);
    backdrop.addSubview(&surface);
    backdrop
}

fn tint(rgb: u32, alpha: f64) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(
        ((rgb >> 16) & 0xff) as f64 / 255.0,
        ((rgb >> 8) & 0xff) as f64 / 255.0,
        (rgb & 0xff) as f64 / 255.0,
        alpha,
    )
}
fn selection_gradient() -> &'static NSGradient {
    static GRADIENT: std::sync::OnceLock<Retained<NSGradient>> = std::sync::OnceLock::new();
    GRADIENT.get_or_init(|| {
        NSGradient::initWithStartingColor_endingColor(
            NSGradient::alloc(),
            &tint(0x2855e8, 1.0),
            &tint(0x087f98, 1.0),
        )
        .unwrap()
    })
}
fn set_label(field: &NSTextField, text: &str) {
    if field.stringValue().to_string() != text {
        field.setStringValue(&NSString::from_str(text));
    }
}
fn label(text: &str, size: f64, frame: NSRect, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setFont(Some(&NSFont::systemFontOfSize(size)));
    label.setFrame(frame);
    label
}
fn demo_windows() -> Vec<WindowInfo> {
    [
        ("Safari", "Rust documentation — ownership and borrowing"),
        ("Visual Studio Code", "main.rs — Winlane"),
        ("Terminal", "cargo test — winlane"),
        (
            "Obsidian",
            tr!("项目笔记 · 窗口切换器", "Project notes · Window switcher"),
        ),
        ("Safari", "AppKit — Apple Developer"),
        ("Finder", "Downloads"),
        ("Notes", tr!("本周计划", "This week's plan")),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (app, title))| WindowInfo {
        id: i as u64,
        pid: if app == "Safari" { -1 } else { -(i as i32 + 2) },
        app: app.into(),
        title: title.into(),
        minimized: i == 5,
    })
    .collect()
}

pub fn run() {
    let mtm = MainThreadMarker::new().expect("Winlane must launch on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    let delegate = Delegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
