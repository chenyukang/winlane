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
    MainThreadMarker, NSBundle, NSData, NSNotification, NSNumber, NSObject, NSObjectProtocol,
    NSPoint, NSRect, NSRunLoop, NSRunLoopCommonModes, NSSize, NSString, NSTimer, NSURL, ns_string,
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
use crate::installed_apps;
use crate::settings::{self, SettingsWindow};
use crate::shortcut_tap::ShortcutTap;

const WIDTH: f64 = 700.0;
const HEIGHT: f64 = 590.0;
const ROW_HEIGHT: f64 = 28.0;
const LIST_TOP: f64 = 484.0;
const LIST_BOTTOM: f64 = 36.0;
const LIST_WIDTH: f64 = WIDTH - 20.0;

struct PanelUi {
    display_id: u32,
    shortcut_label: Retained<NSTextField>,
    scope: Retained<NSButton>,
    panel: Retained<SearchPanel>,
    input: Retained<NSSearchField>,
    scroll: Retained<NSScrollView>,
    list: Retained<ListView>,
    footer: Retained<NSTextField>,
    help: Retained<NSButton>,
    demo_button: Retained<NSButton>,
    refresh_button: Retained<NSButton>,
    mode_label: Retained<NSTextField>,
    actions: Retained<NSPopUpButton>,
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
        if let Some(layer) = self.button.layer() {
            let color = if selected {
                NSColor::selectedContentBackgroundColor()
            } else {
                NSColor::clearColor()
            };
            // SAFETY: CALayer retains the supplied CGColor.
            unsafe {
                let _: () = msg_send![&layer, setBackgroundColor: &*color.CGColor()];
            }
        }
        let text = if selected {
            NSColor::selectedMenuItemTextColor()
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
        let alias = if selected {
            text
        } else if launching {
            NSColor::controlAccentColor()
        } else {
            NSColor::secondaryLabelColor()
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
    check_panel_focus: Cell<bool>,
    config: RefCell<Config>,
    aliases: RefCell<Aliases>,
    identities: RefCell<HashMap<i32, AppIdentity>>,
    icons: RefCell<HashMap<i32, Option<Retained<NSImage>>>>,
    alias_input: RefCell<AliasInput>,
    alias_error: RefCell<Option<String>>,
    aliases_writable: Cell<bool>,
    settings: RefCell<Option<Rc<SettingsWindow>>>,
    app_shortcuts: RefCell<Option<Rc<AppShortcutsWindow>>>,
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
    deferred_windows: RefCell<Option<Vec<WindowInfo>>>,
    hotkey_error: RefCell<Option<String>>,
    windows: RefCell<Vec<WindowInfo>>,
    matches: RefCell<Vec<usize>>,
    launch_matches: RefCell<Vec<usize>>,
    selected: Cell<usize>,
    previous_pid: Cell<i32>,
    receiver: RefCell<Option<Receiver<Vec<WindowInfo>>>>,
    loading: Cell<bool>,
    demo: Cell<bool>,
    recency: RefCell<Vec<u64>>,
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
    // SAFETY: This NSButton subclass customizes cursor rectangles on the main thread.
    #[unsafe(super = NSButton)]
    #[thread_kind = MainThreadOnly]
    #[derive(Debug)]
    struct WindowRowButton;
    unsafe impl NSObjectProtocol for WindowRowButton {}
    impl WindowRowButton {
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
                    self.ivars().aliases.replace(aliases);
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
                if self.ivars().mode.get().is_some() { self.present_panels(); }
            }
        }
    }
    unsafe impl NSWindowDelegate for Delegate {
        #[unsafe(method(windowDidBecomeKey:))]
        fn became_key(&self, _: &NSNotification) {
            self.ivars().check_panel_focus.set(false);
            self.focus_search();
        }
        #[unsafe(method(windowShouldClose:))]
        fn should_close(&self, _: &NSWindow) -> bool { self.dismiss(); false }
        #[unsafe(method(windowDidResignKey:))]
        fn resigned(&self, _: &NSNotification) {
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
            if [sel!(minimizeChosen:), sel!(hideChosen:), sel!(copyTitle:), sel!(quickSelect:)].into_iter().any(|sel| action == Some(sel)) {
                let visible = self.any_panel_visible();
                visible && if action == Some(sel!(quickSelect:)) {
                    (item.tag() as usize) < self.match_count()
                } else { self.selected_window().is_some() }
            } else { true }
        }
    }
    impl Delegate {
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
        #[unsafe(method(saveSettings:))]
        fn save_settings(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.settings_window() {
                match settings.candidate().and_then(|candidate| self.apply_config(candidate)) {
                    Ok(()) => self.report_shortcut_status(),
                    Err(error) => settings.report(&error, true),
                }
            }
        }
        #[unsafe(method(resetSettings:))]
        fn reset_settings(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.settings_window() {
                settings.fill(&Config::default());
                settings.report(tr!("已填入默认值，点击保存后生效。登录启动状态保持不变。", "Defaults filled in. Save to apply. Launch at login is unchanged."), false);
            }
        }
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
            if let Some(window) = self.app_shortcuts_window() { window.remove(sender.tag() as usize); }
        }
        #[unsafe(method(chooseShortcutApp:))]
        fn choose_shortcut_app(&self, sender: &NSButton) {
            if let Some(window) = self.app_shortcuts_window() { window.choose(sender.tag() as usize, self.mtm()); }
        }
        #[unsafe(method(saveAppShortcuts:))]
        fn save_app_shortcuts(&self, _: Option<&AnyObject>) {
            if let Some(window) = self.app_shortcuts_window() {
                let result = window.candidate().and_then(|shortcuts| {
                    let mut config = self.ivars().config.borrow().clone();
                    config.app_shortcuts = shortcuts;
                    self.apply_config(config)
                });
                match result {
                    Ok(()) => window.report(tr!("应用快捷键已保存并启用。", "App shortcuts saved and enabled."), false),
                    Err(error) => window.report(&error, true),
                }
            }
        }
        #[unsafe(method(toggleLogin:))]
        fn toggle_login(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.settings_window() { settings.toggle_login(); }
        }
        #[unsafe(method(manageLogin:))]
        fn manage_login(&self, _: Option<&AnyObject>) { settings::manage_login(); }
        #[unsafe(method(changeScope:))]
        fn change_scope(&self, sender: &NSButton) {
            self.ivars().current_app_only.set(sender.state() == NSControlStateValueOn);
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
            if let Some(settings) = self.settings_window() && settings.window.isKeyWindow() {
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
            let actions: Vec<_> = self.ivars().shortcut_rx.borrow().as_ref()
                .map(|rx| rx.try_iter().collect()).unwrap_or_default();
            for action in actions { self.shortcut_action(action); }
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
        self.ivars().settings.replace(Some(window.clone()));
        window
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
            ui.panel
                .setFrameOrigin(NSPoint::new(position.x, position.y));
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
        state.changing_displays.set(false);
    }

    fn present_panels(&self) {
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
        panel.setTitle(ns_string!("Winlane"));
        panel.setTitleVisibility(NSWindowTitleVisibility::Hidden);
        panel.setTitlebarAppearsTransparent(true);
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
        let root = NSVisualEffectView::initWithFrame(
            NSVisualEffectView::alloc(mtm),
            rect(0.0, 0.0, WIDTH, HEIGHT),
        );
        root.setMaterial(NSVisualEffectMaterial::Popover);
        root.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        root.setState(NSVisualEffectState::Active);
        panel.setContentView(Some(&root));

        let brand = label("Winlane", 13.0, rect(90.0, 556.0, 270.0, 20.0), mtm);
        brand.setTextColor(Some(&NSColor::secondaryLabelColor()));
        root.addSubview(&brand);
        let shortcut = label(
            &self.ivars().config.borrow().shortcut.display(),
            11.0,
            rect(WIDTH - 160.0, 557.0, 144.0, 18.0),
            mtm,
        );
        shortcut.setAlignment(NSTextAlignment::Right);
        shortcut.setTextColor(Some(&NSColor::tertiaryLabelColor()));
        root.addSubview(&shortcut);

        let input_width = ((WIDTH - 32.0) * 0.618).round();
        let input = NSSearchField::initWithFrame(
            NSSearchField::alloc(mtm),
            rect((WIDTH - input_width) / 2.0, 520.0, input_width, 28.0),
        );
        input.setFont(Some(&NSFont::systemFontOfSize(16.0)));
        input.setPlaceholderString(Some(ns_string!("")));
        input.setSendsSearchStringImmediately(true);
        input.setMaximumRecents(0);
        panel.setInitialFirstResponder(Some(&input));
        unsafe {
            input.setDelegate(Some(ProtocolObject::from_ref(self)));
            root.addSubview(&input);
        }
        let scope = self.button(
            tr!("仅当前应用", "Current app only"),
            sel!(changeScope:),
            rect(16.0, 490.0, 180.0, 22.0),
        );
        scope.setButtonType(NSButtonType::Switch);
        scope.setToolTip(Some(&NSString::from_str(tr!(
            "只显示呼出面板前正在使用的应用；演示模式以 Safari 为例。",
            "Only show the app used before opening this panel; Safari is used in the demo."
        ))));
        root.addSubview(&scope);
        let actions = NSPopUpButton::initWithFrame_pullsDown(
            NSPopUpButton::alloc(mtm),
            rect(WIDTH - 146.0, 488.0, 130.0, 26.0),
            true,
        );
        let actions_menu = NSMenu::new(mtm);
        let heading = NSMenuItem::new(mtm);
        heading.setTitle(&NSString::from_str(tr!("窗口操作", "Window Actions")));
        actions_menu.addItem(&heading);
        self.add_window_actions(&actions_menu);
        actions.setMenu(Some(&actions_menu));
        actions.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        root.addSubview(&actions);
        let mode_label = label("", 11.0, rect(16.0, LIST_BOTTOM, WIDTH - 32.0, 20.0), mtm);
        mode_label.setTextColor(Some(&NSColor::secondaryLabelColor()));
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
        footer.setTextColor(Some(&NSColor::secondaryLabelColor()));
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
        root.addSubview(&settings_button);

        Rc::new(PanelUi {
            display_id,
            panel,
            input,
            scroll,
            list,
            footer,
            help,
            demo_button,
            refresh_button: refresh,
            shortcut_label: shortcut,
            scope,
            mode_label,
            actions,
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
            self.report_shortcut_status();
        } else if !state
            .shortcut_tap
            .borrow()
            .as_ref()
            .is_some_and(ShortcutTap::is_enabled)
        {
            state.hotkey_error.replace(Some(
                tr!(
                    "快捷键监听已暂停，请重新保存快捷键设置。",
                    "Shortcut monitoring is paused. Save your shortcuts again."
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

    fn apply_config(&self, candidate: Config) -> Result<(), String> {
        candidate.validate()?;
        let (tap, receiver) = ShortcutTap::new(
            self.mtm(),
            candidate.shortcut.binding()?,
            candidate.switch_shortcut.binding()?,
            candidate.app_bindings()?,
            self.ivars().wake.get().unwrap().handle(),
        )?;
        settings::save(&candidate)?;
        self.ivars().shortcut_tap.replace(Some(tap));
        self.ivars().shortcut_rx.replace(Some(receiver));
        settings::apply_appearance(&candidate, self.mtm());
        if let Some(settings) = self.settings_window() {
            settings.set_app_shortcuts(&candidate.app_shortcuts);
        }
        let language_changed = settings::apply_language(candidate.language);
        self.ivars().config.replace(candidate);
        self.ivars().hotkey_error.replace(None);
        if language_changed {
            self.rebuild_localized_ui();
        }
        self.update_shortcut_labels();
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
        self.end_session();
        self.ivars().launch_receiver.replace(None);
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
        self.ivars().launch_receiver.replace(None);
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
        let workspace = NSWorkspace::sharedWorkspace();
        if let Some(front) = workspace.frontmostApplication()
            && front.processIdentifier() != std::process::id() as i32
            && !self.any_panel_key()
        {
            self.ivars().previous_pid.set(front.processIdentifier());
        }
        self.sync_displays();
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
        self.ensure_app_catalog();
        self.filter_preserving(preserve);
        self.prepare_switch_selection();
        if !self.ivars().demo.get() {
            self.refresh();
        }
        self.present_panels();
    }

    fn focus_search(&self) {
        for ui in self
            .panels()
            .into_iter()
            .filter(|ui| ui.panel.isKeyWindow())
        {
            match self.ivars().mode.get() {
                Some(PanelMode::Search) if ui.input.currentEditor().is_none() => {
                    ui.panel.makeFirstResponder(Some(&ui.input));
                }
                Some(PanelMode::Switch) => {
                    ui.panel.makeFirstResponder(None);
                }
                _ => {}
            }
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
        self.ensure_app_catalog();
        self.filter_preserving(selected_id);
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
            matches
                .iter()
                .position(|&index| windows[index].pid == state.previous_pid.get())
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
        let mut aliases = self.ivars().aliases.borrow_mut();
        if aliases.ensure_windows(windows, &self.ivars().identities.borrow())
            && self.ivars().aliases_writable.get()
        {
            settings::save_aliases(&aliases);
        }
    }

    fn ensure_app_catalog(&self) {
        let state = self.ivars();
        if state.demo.get()
            || state.mode.get() != Some(PanelMode::Search)
            || state.catalog_receiver.borrow().is_some()
            || state
                .catalog_checked
                .get()
                .is_some_and(|at| at.elapsed() < Duration::from_secs(60))
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
        let query = self.ivars().query.borrow();
        if ui.input.stringValue().to_string() != *query {
            ui.input.setStringValue(&NSString::from_str(&query));
        }
        ui.scope.setState(if self.ivars().current_app_only.get() {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
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
        let alias_input = state.alias_input.borrow();
        let alias_query = alias_input.text();
        let aliases = state.aliases.borrow();
        let alias_match = self.alias_match();
        let unmatched_alias =
            switching && !alias_query.is_empty() && alias_match.position().is_none();
        ui.input.setHidden(switching);
        ui.scope.setHidden(switching);
        ui.actions.setHidden(switching);
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
        let list_top = if switching { 548.0 } else { LIST_TOP };
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
            let heading = label(title, 21.0, rect(40.0, 90.0, 540.0, 38.0), self.mtm());
            let detail = label(detail, 14.0, rect(40.0, 137.0, 550.0, 96.0), self.mtm());
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
        let button: Retained<WindowRowButton> =
            unsafe { msg_send![WindowRowButton::alloc(mtm), initWithFrame: frame] };
        button.setTitle(ns_string!(""));
        button.setBordered(false);
        button.setTag(position as isize);
        // SAFETY: The delegate outlives its rows and pickWindow: takes a button sender.
        unsafe {
            button.setTarget(Some(self));
            button.setAction(Some(sel!(pickWindow:)));
        }
        button.setWantsLayer(true);
        if let Some(layer) = button.layer() {
            unsafe {
                let _: () = msg_send![&layer, setCornerRadius: 5.0f64];
            }
        }
        let title = label("", 13.0, rect(222.0, 3.0, LIST_WIDTH - 236.0, 20.0), mtm);
        let app = label("", 13.0, rect(42.0, 3.0, 142.0, 20.0), mtm);
        app.setAlignment(NSTextAlignment::Right);
        for field in [&title, &app] {
            field.setMaximumNumberOfLines(1);
            field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
            button.addSubview(field);
        }
        let alias = label("", 12.0, rect(8.0, 3.0, 28.0, 20.0), mtm);
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
        let mut recent = self.ivars().recency.borrow_mut();
        recent.retain(|id| *id != window.id);
        recent.insert(0, window.id);
        recent.truncate(128);
        drop(recent);
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
