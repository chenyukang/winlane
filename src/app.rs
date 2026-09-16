use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::*;
use objc2_foundation::{
    MainThreadMarker, NSBundle, NSNotification, NSNumber, NSObject, NSObjectProtocol, NSPoint,
    NSRect, NSRunLoop, NSRunLoopCommonModes, NSSize, NSString, NSTimer, NSURL, ns_string,
};
use winlane::aliases::{AliasInput, Aliases, AppIdentity, matching_alias_position};
use winlane::config::{Config, visible_matches};
use winlane::displays::{Display, Rect, placements};
use winlane::search::WindowInfo;
use winlane::shortcuts::{
    Action, ActionKind, PanelCommand, PanelMode, SPACE, SwitchSelection, panel_command,
    space_changes_mode,
};

use crate::accessibility;
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
    alias_input: RefCell<AliasInput>,
    alias_error: RefCell<Option<String>>,
    aliases_writable: Cell<bool>,
    settings: OnceCell<SettingsWindow>,
    show_menu_item: OnceCell<Retained<NSMenuItem>>,
    status_item: OnceCell<Retained<NSStatusItem>>,
    timer: OnceCell<Retained<NSTimer>>,
    shortcut_tap: RefCell<Option<ShortcutTap>>,
    shortcut_rx: RefCell<Option<Receiver<Action>>>,
    shortcut_check_tick: Cell<u32>,
    mode: Cell<Option<PanelMode>>,
    session: Cell<u64>,
    switch_selection: RefCell<Option<SwitchSelection>>,
    switch_anchor: Cell<Option<u64>>,
    deferred_windows: RefCell<Option<Vec<WindowInfo>>>,
    hotkey_error: RefCell<Option<String>>,
    windows: RefCell<Vec<WindowInfo>>,
    matches: RefCell<Vec<usize>>,
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
    // SAFETY: All AppKit state is main-thread-only and lives until the application terminates.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = AppState]
    struct Delegate;
    unsafe impl NSObjectProtocol for Delegate {}
    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidBecomeActive:))]
        fn became_active(&self, _: &NSNotification) {
            if let Some(settings) = self.ivars().settings.get() { settings.update_login_status(); }
        }
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_launch(&self, _: &NSNotification) {
            match settings::load() {
                Ok(config) => { self.ivars().config.replace(config); }
                Err(error) => { self.ivars().hotkey_error.replace(Some(error)); }
            }
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
            if self.ivars().status_item.get().is_some() {
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
                    (item.tag() as usize) < self.ivars().matches.borrow().len()
                } else { self.selected_window().is_some() }
            } else { true }
        }
    }
    impl Delegate {
        #[unsafe(method(showSettings:))]
        fn settings_action(&self, _: Option<&AnyObject>) {
            self.cancel_routing();
            self.end_session();
            let settings = self.ivars().settings.get_or_init(|| SettingsWindow::new(self, self.mtm()));
            NSApplication::sharedApplication(self.mtm()).activate();
            settings.show(&self.ivars().config.borrow());
            self.report_shortcut_status();
        }
        #[unsafe(method(saveSettings:))]
        fn save_settings(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.ivars().settings.get() {
                match settings.candidate().and_then(|candidate| self.apply_config(candidate)) {
                    Ok(()) => self.report_shortcut_status(),
                    Err(error) => settings.report(&error, true),
                }
            }
        }
        #[unsafe(method(resetSettings:))]
        fn reset_settings(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.ivars().settings.get() {
                settings.fill(&Config::default());
                settings.report("已填入默认值，点击保存后生效。登录启动状态保持不变。", false);
            }
        }
        #[unsafe(method(toggleLogin:))]
        fn toggle_login(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.ivars().settings.get() { settings.toggle_login(); }
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
                    self.report_switch_error("已复制窗口标题。");
                } else { self.report_switch_error("无法写入剪贴板，请重试。"); }
            }
        }
        #[unsafe(method(quickSelect:))]
        fn quick_select(&self, sender: &NSMenuItem) {
            if !self.any_panel_visible() { return; }
            if (sender.tag() as usize) < self.ivars().matches.borrow().len() {
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
            if let Some(settings) = self.ivars().settings.get() && settings.window.isKeyWindow() {
                settings.window.close();
            } else if self.any_panel_key() { self.dismiss(); }
        }
        #[unsafe(method(refreshWindows:))]
        fn refresh_action(&self, _: Option<&AnyObject>) {
            if self.ivars().demo.get() { self.filter(); } else { self.refresh(); }
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
        fn poll(&self, _: &NSTimer) {
            if self.ivars().check_panel_focus.replace(false)
                && !self.ivars().changing_displays.get()
                && !self.any_panel_key()
            { self.end_session(); }
            self.check_shortcuts();
            let actions: Vec<_> = self.ivars().shortcut_rx.borrow().as_ref()
                .map(|rx| rx.try_iter().collect()).unwrap_or_default();
            for action in actions { self.shortcut_action(action); }
            let result = self.ivars().receiver.borrow().as_ref().map(|rx| rx.try_recv());
            if let Some(Ok(mut windows)) = result {
                self.ivars().receiver.replace(None);
                self.ivars().loading.set(false);
                if !self.ivars().demo.get() {
                    if !accessibility::is_trusted() { windows.clear(); }
                    self.update_aliases(&windows);
                    let snapshot_ready = self.ivars().switch_selection.borrow().as_ref()
                        .is_some_and(|selection| selection.selected().is_some());
                    if self.ivars().mode.get() == Some(PanelMode::Switch) && snapshot_ready {
                        self.ivars().deferred_windows.replace(Some(windows));
                        self.select_alias();
                        self.render();
                        self.commit_switch_if_ready();
                    } else {
                        let selected_id = self.selected_window().map(|window| window.id);
                        self.ivars().windows.replace(windows);
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
                self.report_switch_error("读取窗口失败，请按 ⌘R 重试。");
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
        unsafe { msg_send![super(this), init] }
    }

    fn build_ui(&self) {
        let mtm = self.mtm();
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        let main_menu = NSMenu::new(mtm);
        let application_item = NSMenuItem::new(mtm);
        let application_menu = NSMenu::new(mtm);
        // SAFETY: quitApp: is implemented by this retained delegate.
        let quit_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("退出 Winlane"),
                Some(sel!(quitApp:)),
                ns_string!("q"),
            )
        };
        unsafe { quit_item.setTarget(Some(self)) };
        application_menu.addItem(&quit_item);
        let settings_item = self.menu_item("设置…", sel!(showSettings:), ",");
        application_menu.insertItem_atIndex(&settings_item, 0);
        application_menu
            .insertItem_atIndex(&self.menu_item("打开窗口搜索", sel!(showSearch:), ""), 0);
        application_item.setSubmenu(Some(&application_menu));
        main_menu.addItem(&application_item);
        let edit_menu = NSMenu::new(mtm);
        let edit_item = NSMenuItem::new(mtm);
        edit_item.setTitle(ns_string!("编辑"));
        for (title, action, key) in [
            ("剪切", sel!(cut:), "x"),
            ("拷贝", sel!(copy:), "c"),
            ("粘贴", sel!(paste:), "v"),
            ("全选", sel!(selectAll:), "a"),
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
        window_item.setTitle(ns_string!("窗口"));
        let window_menu = NSMenu::new(mtm);
        window_menu.addItem(&self.menu_item("关闭窗口", sel!(closeWindow:), "w"));
        window_menu.addItem(&NSMenuItem::separatorItem(mtm));
        self.add_window_actions(&window_menu);
        window_menu.addItem(&NSMenuItem::separatorItem(mtm));
        window_menu.addItem(&self.menu_item("刷新窗口", sel!(refreshWindows:), "r"));
        for index in 0..9 {
            let item = self.menu_item(
                &format!("切换到第 {} 个窗口", index + 1),
                sel!(quickSelect:),
                &(index + 1).to_string(),
            );
            item.setTag(index);
            window_menu.addItem(&item);
        }
        window_item.setSubmenu(Some(&window_menu));
        main_menu.addItem(&window_item);
        app.setMainMenu(Some(&main_menu));
        self.sync_displays();

        let status_item =
            NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        if let Some(button) = status_item.button(mtm) {
            button.setTitle(ns_string!("▤"));
            button.setToolTip(Some(ns_string!("Winlane · 窗口搜索")));
        }
        let menu = NSMenu::new(mtm);
        for (title, action, key) in [
            ("打开窗口搜索", sel!(showSearch:), ""),
            ("打开窗口切换", sel!(showSwitcher:), ""),
            ("设置…", sel!(showSettings:), ","),
            ("刷新窗口列表", sel!(refreshWindows:), ""),
            ("辅助功能设置…", sel!(openPermissions:), ""),
            ("退出 Winlane", sel!(quitApp:), "q"),
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
                self.ivars().show_menu_item.set(item).unwrap();
            }
        }
        status_item.setMenu(Some(&menu));
        self.ivars().status_item.set(status_item).unwrap();
        self.update_shortcut_labels();
        // SAFETY: The application retains this delegate for the entire run loop; poll: has NSTimer signature.
        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                0.04,
                self,
                sel!(poll:),
                None,
                true,
            )
        };
        timer.setTolerance(0.005);
        // SAFETY: Polling also runs during menu tracking; otherwise queued
        // shortcut events can wait until a menu or modal interaction ends.
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
        self.ivars().timer.set(timer).unwrap();
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

        let input = NSSearchField::initWithFrame(
            NSSearchField::alloc(mtm),
            rect(16.0, 520.0, WIDTH - 32.0, 28.0),
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
            "仅当前应用",
            sel!(changeScope:),
            rect(16.0, 490.0, 180.0, 22.0),
        );
        scope.setButtonType(NSButtonType::Switch);
        scope.setToolTip(Some(ns_string!(
            "只显示呼出面板前正在使用的应用；演示模式以 Safari 为例。"
        )));
        root.addSubview(&scope);
        let actions = NSPopUpButton::initWithFrame_pullsDown(
            NSPopUpButton::alloc(mtm),
            rect(WIDTH - 146.0, 488.0, 130.0, 26.0),
            true,
        );
        let actions_menu = NSMenu::new(mtm);
        let heading = NSMenuItem::new(mtm);
        heading.setTitle(ns_string!("窗口操作"));
        actions_menu.addItem(&heading);
        self.add_window_actions(&actions_menu);
        actions.setMenu(Some(&actions_menu));
        actions.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        root.addSubview(&actions);
        let mode_label = label("", 13.0, rect(16.0, 524.0, WIDTH - 32.0, 22.0), mtm);
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
            "正在准备窗口列表…",
            11.0,
            rect(16.0, 9.0, WIDTH - 128.0, 18.0),
            mtm,
        );
        footer.setTextColor(Some(&NSColor::secondaryLabelColor()));
        root.addSubview(&footer);
        let help = self.button(
            "辅助功能设置…",
            sel!(openPermissions:),
            rect(16.0, 34.0, 138.0, 25.0),
        );
        root.addSubview(&help);
        let demo_button = self.button(
            "查看演示",
            sel!(toggleDemo:),
            rect(164.0, 34.0, 100.0, 25.0),
        );
        root.addSubview(&demo_button);
        let refresh = self.button(
            "刷新 ↻",
            sel!(refreshWindows:),
            rect(WIDTH - 92.0, 34.0, 76.0, 25.0),
        );
        refresh.setHidden(accessibility::is_trusted());
        root.addSubview(&refresh);
        let settings_button = self.button(
            "设置…  ⌘,",
            sel!(showSettings:),
            rect(WIDTH - 106.0, 6.0, 90.0, 24.0),
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
        let tick = state.shortcut_check_tick.get().wrapping_add(1);
        state.shortcut_check_tick.set(tick);
        if !tick.is_multiple_of(25) {
            return;
        }
        if !accessibility::is_trusted() {
            if state.shortcut_tap.borrow().is_some() {
                self.end_session();
                state.shortcut_tap.replace(None);
                state.shortcut_rx.replace(None);
                state.hotkey_error.replace(Some(
                    "全局快捷键未启用，请在系统设置中重新允许 Winlane。".into(),
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
            state
                .hotkey_error
                .replace(Some("快捷键监听已暂停，请重新保存快捷键设置。".into()));
            self.report_shortcut_status();
        }
    }

    fn report_shortcut_status(&self) {
        if let Some(settings) = self.ivars().settings.get() {
            if let Some(error) = self.ivars().hotkey_error.borrow().as_deref() {
                settings.report(error, true);
            } else {
                let config = self.ivars().config.borrow();
                settings.report(
                    &format!(
                        "搜索 {} · 切换 {} 已启用。",
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
        )?;
        settings::save(&candidate)?;
        self.ivars().shortcut_tap.replace(Some(tap));
        self.ivars().shortcut_rx.replace(Some(receiver));
        settings::apply_appearance(&candidate, self.mtm());
        self.ivars().config.replace(candidate);
        self.ivars().hotkey_error.replace(None);
        self.update_shortcut_labels();
        self.filter();
        Ok(())
    }

    fn update_shortcut_labels(&self) {
        let display = self.ivars().config.borrow().shortcut.display();
        self.ivars()
            .show_menu_item
            .get()
            .unwrap()
            .setTitle(&NSString::from_str(&format!("打开窗口搜索    {display}")));
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
        menu.addItem(&self.menu_item("最小化 / 恢复所选窗口", sel!(minimizeChosen:), "m"));
        menu.addItem(&self.menu_item("隐藏所选应用", sel!(hideChosen:), "h"));
        let copy = self.menu_item("复制窗口标题", sel!(copyTitle:), "c");
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
        if let Some(settings) = self.ivars().settings.get() {
            settings.window.orderOut(None);
        }
        let preserve = self.selected_window().map(|window| window.id);
        let deferred = self.ivars().deferred_windows.borrow_mut().take();
        if let Some(windows) = deferred {
            self.ivars().windows.replace(windows);
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
        self.ivars()
            .switch_anchor
            .set(if direction == 0 { preserve } else { None });
        if mode == PanelMode::Switch {
            self.ivars().current_app_only.set(false);
        }
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
            state.windows.replace(windows);
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
        let selected_id = self.selected_window().map(|window| window.id);
        state.session.set(session);
        state.mode.set(Some(PanelMode::Search));
        state.alias_input.borrow_mut().clear();
        state.switch_selection.replace(None);
        let deferred = state.deferred_windows.borrow_mut().take();
        if let Some(windows) = deferred {
            state.windows.replace(windows);
        }
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
        let state = self.ivars();
        let windows = state.windows.borrow();
        let identities = state.identities.borrow();
        let ordered_apps: Vec<_> = state
            .matches
            .borrow()
            .iter()
            .map(|&index| {
                identities
                    .get(&windows[index].pid)
                    .map_or("", |app| app.id.as_str())
            })
            .collect();
        matching_alias_position(
            &state.aliases.borrow(),
            state.alias_input.borrow().text(),
            &ordered_apps,
        )
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
                if let Some(identity) = app_identity(&app) {
                    state
                        .identities
                        .borrow_mut()
                        .insert(app.processIdentifier(), identity);
                }
                (
                    app.processIdentifier(),
                    app.localizedName()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "Application".into()),
                )
            })
            .collect::<Vec<_>>();
        state.loading.set(true);
        let (tx, rx) = mpsc::channel();
        state.receiver.replace(Some(rx));
        std::thread::spawn(move || {
            let _ = tx.send(accessibility::list_windows(&apps));
        });
        self.render();
    }

    fn update_aliases(&self, windows: &[WindowInfo]) {
        let apps: Vec<_> = windows
            .iter()
            .filter_map(|window| self.ivars().identities.borrow().get(&window.pid).cloned())
            .collect();
        let mut aliases = self.ivars().aliases.borrow_mut();
        if aliases.ensure(&apps) && self.ivars().aliases_writable.get() {
            settings::save_aliases(&aliases);
        }
    }

    fn filter(&self) {
        self.filter_preserving(None);
    }

    fn filter_preserving(&self, selected_id: Option<u64>) {
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
        let is_alias = aliases.resolve(&query).is_some();
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
            .filter_order(
                &query,
                &matched,
                &windows,
                &self.ivars().identities.borrow(),
            )
            .unwrap_or(matched);
        drop(aliases);
        let selected = selected_id
            .and_then(|id| matched.iter().position(|&index| windows[index].id == id))
            .unwrap_or(0);
        drop(windows);
        self.ivars().matches.replace(matched);
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
        let count = self.ivars().matches.borrow().len();
        if count == 0 {
            return;
        }
        self.ivars().selected.set(
            (self.ivars().selected.get() as isize + direction).rem_euclid(count as isize) as usize,
        );
        self.render();
    }

    fn render(&self) {
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
        for subview in list.subviews() {
            subview.removeFromSuperview();
        }
        let windows = state.windows.borrow();
        let matched = state.matches.borrow();
        let trusted = accessibility::is_trusted();
        let demo = state.demo.get();
        let switching = state.mode.get() == Some(PanelMode::Switch);
        let alias_input = state.alias_input.borrow();
        let alias_query = alias_input.text();
        let aliases = state.aliases.borrow();
        let identities = state.identities.borrow();
        let unmatched_alias =
            switching && !alias_query.is_empty() && self.alias_position().is_none();
        ui.input.setHidden(switching);
        ui.scope.setHidden(switching);
        ui.actions.setHidden(switching);
        ui.mode_label.setHidden(!switching);
        let config = state.config.borrow();
        ui.mode_label
            .setStringValue(&NSString::from_str(&if alias_query.is_empty() {
                format!(
                    "切换模式 · {} 选择 · 松开 {} 或 ↵ 确认",
                    config.switch_shortcut.display(),
                    config.switch_shortcut.release_label()
                )
            } else {
                format!(
                    "Alias  {alias_query}  ·  {}",
                    if unmatched_alias {
                        if aliases.has_prefix(alias_query) && aliases.resolve(alias_query).is_none()
                        {
                            "继续输入第二个字母"
                        } else {
                            "没有匹配窗口 · Backspace 修改"
                        }
                    } else {
                        "松开修饰键确认 · Backspace 修改"
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
        ui.demo_button.setTitle(if demo {
            ns_string!("返回真实窗口")
        } else {
            ns_string!("查看演示")
        });
        let list_bottom = if !trusted || demo { 64.0 } else { LIST_BOTTOM };
        let list_top = if switching { 516.0 } else { LIST_TOP };
        let list_height = list_top - list_bottom;
        ui.scroll
            .setFrame(rect(10.0, list_bottom, LIST_WIDTH, list_height));
        list.setFrameSize(NSSize::new(
            LIST_WIDTH,
            (matched.len() as f64 * ROW_HEIGHT).max(list_height),
        ));
        if matched.is_empty() {
            let (title, detail) = if state.loading.get() {
                ("正在读取窗口…", "窗口枚举在后台进行，搜索界面仍可输入。")
            } else if !trusted && !demo {
                (
                    "先允许 Winlane 控制窗口",
                    "在系统设置 → 隐私与安全性的应用控制权限中启用 Winlane。\nmacOS 27：Device Control and Data Access；旧版：辅助功能。\n用于读取与切换窗口。授权后重新呼出搜索面板即可。",
                )
            } else {
                (
                    "没有匹配的窗口",
                    "试试其他关键词，或检查当前应用筛选和排除设置。",
                )
            };
            let heading = label(title, 21.0, rect(40.0, 90.0, 540.0, 38.0), self.mtm());
            let detail = label(detail, 14.0, rect(40.0, 137.0, 550.0, 96.0), self.mtm());
            detail.setTextColor(Some(&NSColor::secondaryLabelColor()));
            detail.setMaximumNumberOfLines(4);
            list.addSubview(&heading);
            list.addSubview(&detail);
        }
        for (position, &index) in matched.iter().enumerate() {
            let item = &windows[index];
            let selected = position == state.selected.get() && !unmatched_alias;
            let row = self.button(
                "",
                sel!(pickWindow:),
                rect(
                    2.0,
                    position as f64 * ROW_HEIGHT + 1.0,
                    LIST_WIDTH - 4.0,
                    ROW_HEIGHT - 2.0,
                ),
            );
            row.setTag(position as isize);
            row.setAccessibilitySelected(selected);
            row.setBordered(false);
            row.setWantsLayer(true);
            if selected && let Some(layer) = row.layer() {
                let color = NSColor::selectedContentBackgroundColor().CGColor();
                // SAFETY: CALayer accepts a CGColor reference and a CGFloat (f64 on macOS).
                unsafe {
                    let _: () = msg_send![&layer, setBackgroundColor: &*color];
                    let _: () = msg_send![&layer, setCornerRadius: 5.0f64];
                }
            }
            let title = if item.title.trim().is_empty() {
                item.app.as_str()
            } else {
                item.title.as_str()
            };
            let alias = identities
                .get(&item.pid)
                .and_then(|app| aliases.get(&app.id));
            let tooltip = format!(
                "{} — {}{}",
                item.app,
                title,
                alias.map_or(String::new(), |alias| format!(" · alias {alias}"))
            );
            row.setToolTip(Some(&NSString::from_str(&tooltip)));
            row.setAccessibilityLabel(Some(&NSString::from_str(&tooltip)));
            let title = if item.minimized {
                format!("{title} · 已最小化")
            } else {
                title.to_owned()
            };
            let title_field = label(
                &title,
                13.0,
                rect(222.0, 3.0, LIST_WIDTH - 236.0, 20.0),
                self.mtm(),
            );
            title_field.setMaximumNumberOfLines(1);
            title_field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
            let subtitle_field = label(&item.app, 13.0, rect(42.0, 3.0, 142.0, 20.0), self.mtm());
            subtitle_field.setAlignment(NSTextAlignment::Right);
            subtitle_field.setMaximumNumberOfLines(1);
            subtitle_field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
            subtitle_field.setTextColor(Some(&NSColor::labelColor()));
            if selected {
                title_field.setTextColor(Some(&NSColor::selectedMenuItemTextColor()));
                subtitle_field.setTextColor(Some(&NSColor::selectedMenuItemTextColor()));
            }
            let icon = NSImageView::initWithFrame(
                NSImageView::alloc(self.mtm()),
                rect(192.0, 3.0, 20.0, 20.0),
            );
            icon.setImageScaling(NSImageScaling::ScaleProportionallyDown);
            let image = NSRunningApplication::runningApplicationWithProcessIdentifier(item.pid)
                .and_then(|app| app.icon())
                .or_else(|| {
                    NSImage::imageWithSystemSymbolName_accessibilityDescription(
                        ns_string!("macwindow"),
                        Some(ns_string!("窗口")),
                    )
                });
            icon.setImage(image.as_deref());
            row.addSubview(&icon);
            row.addSubview(&title_field);
            row.addSubview(&subtitle_field);
            if let Some(alias) = alias {
                let badge = label(alias, 12.0, rect(8.0, 3.0, 28.0, 20.0), self.mtm());
                badge.setAlignment(NSTextAlignment::Left);
                let color = if selected {
                    NSColor::selectedMenuItemTextColor()
                } else {
                    NSColor::secondaryLabelColor()
                };
                badge.setTextColor(Some(&color));
                row.addSubview(&badge);
            }
            list.addSubview(&row);
            if selected {
                row.scrollRectToVisible(row.bounds());
            }
        }
        let status = if demo {
            format!(
                "演示模式 · {} 个示例 · ↑↓ 选择  ↵ 预览选择  Esc 关闭",
                matched.len()
            )
        } else if let Some(error) = state.hotkey_error.borrow().as_ref() {
            error.clone()
        } else if let Some(error) = state.alias_error.borrow().as_ref() {
            error.clone()
        } else if !trusted {
            "需要辅助功能权限；也可以先查看演示。".into()
        } else if state.loading.get() {
            "正在刷新窗口…".into()
        } else if switching {
            format!(
                "{} 项 · 字母定位 · ↑↓ 选择 · Space 搜索 · Esc 取消",
                matched.len()
            )
        } else {
            format!(
                "{} 项 · 输入 alias 或标题 · ↵ 切换 · Space 切换模式",
                matched.len()
            )
        };
        ui.footer.setStringValue(&NSString::from_str(&status));
    }

    fn activate_app(&self, app: &NSRunningApplication) -> bool {
        let current = NSRunningApplication::currentApplication();
        NSApplication::sharedApplication(self.mtm()).yieldActivationToApplication(app);
        app.activateFromApplication_options(&current, NSApplicationActivationOptions::empty())
    }

    fn activate_selected(&self) {
        if let Some(tap) = self.ivars().shortcut_tap.borrow().as_ref() {
            tap.finish(self.ivars().session.get());
        }
        self.ivars().switch_selection.replace(None);
        let Some(window) = self.selected_window() else {
            self.selection_failed("没有匹配的窗口，请重新搜索。");
            return;
        };
        if self.ivars().demo.get() {
            self.selection_failed(&format!(
                "演示选择：{} · {}（未切换真实窗口）",
                window.app, window.title
            ));
            return;
        }
        let Some(target_app) =
            NSRunningApplication::runningApplicationWithProcessIdentifier(window.pid)
        else {
            self.selection_failed("应用已退出，请按 ⌘R 更新窗口列表。");
            return;
        };
        target_app.unhide();
        if let Err(error) = accessibility::raise_window(window.pid, window.id) {
            self.selection_failed(&error);
            return;
        }
        if !self.activate_app(&target_app) {
            self.selection_failed("系统未接受切换请求，请重试或检查辅助功能权限。");
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
        self.filter_preserving(Some(window.id));
        self.report_switch_error(if self.ivars().demo.get() {
            "演示：已更新示例窗口的最小化状态，未操作真实窗口。"
        } else if minimized {
            "已最小化窗口。"
        } else {
            "已恢复窗口。"
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
            self.report_switch_error(&format!("演示：隐藏 {}（未操作真实应用）。", window.app));
            return;
        }
        match NSRunningApplication::runningApplicationWithProcessIdentifier(window.pid) {
            Some(app) if app.hide() => {
                self.report_switch_error("已隐藏应用。选择它的窗口后按回车可重新打开。")
            }
            _ => self.report_switch_error("无法隐藏应用，它可能已经退出。"),
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
        ("Obsidian", "项目笔记 · 窗口切换器"),
        ("Safari", "AppKit — Apple Developer"),
        ("Finder", "Downloads"),
        ("Notes", "本周计划"),
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
