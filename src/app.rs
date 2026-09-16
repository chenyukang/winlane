use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::*;
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSRunLoop,
    NSRunLoopCommonModes, NSSize, NSString, NSTimer, NSURL, ns_string,
};
use winlane::config::{Config, Shortcut, visible_matches};
use winlane::search::WindowInfo;
use winlane::shortcuts::{CommandTabAction, PanelCommand, PressLatch, panel_command};

use crate::accessibility;
use crate::command_tab::CommandTabTap;
use crate::settings::{self, SettingsWindow};

const WIDTH: f64 = 660.0;
const HEIGHT: f64 = 590.0;
const ROW_HEIGHT: f64 = 64.0;
const LIST_HEIGHT: f64 = 372.0;

#[derive(Default)]
struct AppState {
    config: RefCell<Config>,
    settings: OnceCell<SettingsWindow>,
    shortcut_label: OnceCell<Retained<NSTextField>>,
    show_menu_item: OnceCell<Retained<NSMenuItem>>,
    scope: OnceCell<Retained<NSButton>>,
    panel: OnceCell<Retained<SearchPanel>>,
    input: OnceCell<Retained<NSSearchField>>,
    list: OnceCell<Retained<ListView>>,
    footer: OnceCell<Retained<NSTextField>>,
    help: OnceCell<Retained<NSButton>>,
    demo_button: OnceCell<Retained<NSButton>>,
    refresh_button: OnceCell<Retained<NSButton>>,
    status_item: OnceCell<Retained<NSStatusItem>>,
    timer: OnceCell<Retained<NSTimer>>,
    hotkey_manager: RefCell<Option<GlobalHotKeyManager>>,
    hotkey: Cell<Option<HotKey>>,
    hotkey_press: RefCell<PressLatch>,
    command_tab: RefCell<Option<CommandTabTap>>,
    command_tab_rx: RefCell<Option<Receiver<CommandTabAction>>>,
    shortcut_check_tick: Cell<u32>,
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
            settings::apply_appearance(&self.ivars().config.borrow(), self.mtm());
            self.build_ui();
            self.register_hotkey();
            if !accessibility::is_trusted() { self.show(); }
        }
        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn reopen(&self, _: &NSApplication, _: bool) -> bool { self.show(); true }
    }
    unsafe impl NSWindowDelegate for Delegate {
        #[unsafe(method(windowDidBecomeKey:))]
        fn became_key(&self, _: &NSNotification) {
            self.focus_search();
        }
        #[unsafe(method(windowShouldClose:))]
        fn should_close(&self, _: &NSWindow) -> bool { self.dismiss(); false }
        #[unsafe(method(windowDidResignKey:))]
        fn resigned(&self, _: &NSNotification) {
            if let Some(panel) = self.ivars().panel.get() { panel.orderOut(None); }
        }
    }
    unsafe impl NSControlTextEditingDelegate for Delegate {
        #[unsafe(method(controlTextDidChange:))]
        fn text_changed(&self, _: &NSNotification) { self.filter(); }
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
                let visible = self.ivars().panel.get().is_some_and(|panel| panel.isVisible());
                visible && if action == Some(sel!(quickSelect:)) {
                    (item.tag() as usize) < self.ivars().matches.borrow().len()
                } else { self.selected_window().is_some() }
            } else { true }
        }
    }
    impl Delegate {
        #[unsafe(method(showSettings:))]
        fn settings_action(&self, _: Option<&AnyObject>) {
            self.ivars().panel.get().unwrap().orderOut(None);
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
        fn change_scope(&self, _: Option<&AnyObject>) { self.filter(); }
        #[unsafe(method(minimizeChosen:))]
        fn minimize_chosen(&self, _: Option<&AnyObject>) { self.minimize_selected(); }
        #[unsafe(method(hideChosen:))]
        fn hide_chosen(&self, _: Option<&AnyObject>) { self.hide_selected(); }
        #[unsafe(method(copyTitle:))]
        fn copy_title(&self, _: Option<&AnyObject>) {
            if !self.ivars().panel.get().unwrap().isVisible() { return; }
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
            if !self.ivars().panel.get().unwrap().isVisible() { return; }
            if (sender.tag() as usize) < self.ivars().matches.borrow().len() {
                self.ivars().selected.set(sender.tag() as usize);
                self.render(); self.activate_selected();
            }
        }
        #[unsafe(method(showSearch:))]
        fn show_action(&self, _: Option<&AnyObject>) { self.show(); }
        #[unsafe(method(closeWindow:))]
        fn close_window(&self, _: Option<&AnyObject>) {
            if let Some(settings) = self.ivars().settings.get() && settings.window.isKeyWindow() {
                settings.window.close();
            } else if self.ivars().panel.get().unwrap().isKeyWindow() { self.dismiss(); }
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
            state.input.get().unwrap().setStringValue(ns_string!(""));
            if state.demo.get() {
                state.windows.replace(demo_windows());
                state.loading.set(false);
                self.filter();
            } else { state.windows.borrow_mut().clear(); self.refresh(); }
        }
        #[unsafe(method(poll:))]
        fn poll(&self, _: &NSTimer) {
            self.check_command_tab();
            let actions: Vec<_> = self.ivars().command_tab_rx.borrow().as_ref()
                .map(|rx| rx.try_iter().collect()).unwrap_or_default();
            for action in actions { self.command_tab_action(action); }
            while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                if self.ivars().hotkey.get().is_some_and(|key| key.id() == event.id) {
                    let first_press = self.ivars().hotkey_press.borrow_mut().update(event.state == HotKeyState::Pressed);
                    if first_press {
                        let panel = self.ivars().panel.get().unwrap();
                        if panel.isKeyWindow() { self.dismiss(); } else { self.show(); }
                    }
                }
            }
            let result = self.ivars().receiver.borrow().as_ref().map(|rx| rx.try_recv());
            if let Some(Ok(mut windows)) = result {
                self.ivars().receiver.replace(None);
                self.ivars().loading.set(false);
                if !self.ivars().demo.get() {
                    if !accessibility::is_trusted() { windows.clear(); }
                    let selected_id = self.selected_window().map(|window| window.id);
                    self.ivars().windows.replace(windows);
                    self.filter_preserving(selected_id);
                }
            } else if matches!(result, Some(Err(TryRecvError::Disconnected))) {
                self.ivars().receiver.replace(None);
                self.ivars().loading.set(false);
                self.render();
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
            NSWindowCollectionBehavior::MoveToActiveSpace
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

        let brand = label("Winlane", 15.0, rect(90.0, 546.0, 270.0, 25.0), mtm);
        brand.setTextColor(Some(&NSColor::secondaryLabelColor()));
        root.addSubview(&brand);
        let shortcut = label(
            &self.ivars().config.borrow().shortcut.display(),
            12.0,
            rect(420.0, 548.0, 206.0, 21.0),
            mtm,
        );
        shortcut.setAlignment(NSTextAlignment::Right);
        shortcut.setTextColor(Some(&NSColor::tertiaryLabelColor()));
        root.addSubview(&shortcut);
        self.ivars().shortcut_label.set(shortcut).unwrap();

        let input = NSSearchField::initWithFrame(
            NSSearchField::alloc(mtm),
            rect(22.0, 492.0, WIDTH - 44.0, 38.0),
        );
        input.setFont(Some(&NSFont::systemFontOfSize(20.0)));
        input.setPlaceholderString(Some(ns_string!("搜索应用或窗口标题…")));
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
            rect(22.0, 454.0, 280.0, 25.0),
        );
        scope.setButtonType(NSButtonType::Switch);
        scope.setToolTip(Some(ns_string!(
            "只显示呼出面板前正在使用的应用；演示模式以 Safari 为例。"
        )));
        root.addSubview(&scope);
        self.ivars().scope.set(scope).unwrap();
        let actions = NSPopUpButton::initWithFrame_pullsDown(
            NSPopUpButton::alloc(mtm),
            rect(480.0, 451.0, 160.0, 29.0),
            true,
        );
        let actions_menu = NSMenu::new(mtm);
        let heading = NSMenuItem::new(mtm);
        heading.setTitle(ns_string!("窗口操作"));
        actions_menu.addItem(&heading);
        self.add_window_actions(&actions_menu);
        actions.setMenu(Some(&actions_menu));
        root.addSubview(&actions);

        let scroll = NSScrollView::initWithFrame(
            NSScrollView::alloc(mtm),
            rect(10.0, 76.0, WIDTH - 20.0, LIST_HEIGHT),
        );
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setDrawsBackground(false);
        scroll.setBorderType(NSBorderType::NoBorder);
        let list: Retained<ListView> = unsafe {
            msg_send![ListView::alloc(mtm), initWithFrame: rect(0.0, 0.0, WIDTH - 36.0, LIST_HEIGHT)]
        };
        scroll.setDocumentView(Some(&list));
        root.addSubview(&scroll);

        let footer = label(
            "正在准备窗口列表…",
            12.0,
            rect(24.0, 43.0, 480.0, 24.0),
            mtm,
        );
        footer.setTextColor(Some(&NSColor::secondaryLabelColor()));
        root.addSubview(&footer);
        let help = self.button(
            "辅助功能设置…",
            sel!(openPermissions:),
            rect(22.0, 12.0, 138.0, 25.0),
        );
        root.addSubview(&help);
        let demo_button = self.button(
            "查看演示",
            sel!(toggleDemo:),
            rect(170.0, 12.0, 100.0, 25.0),
        );
        root.addSubview(&demo_button);
        let refresh = self.button(
            "刷新 ↻",
            sel!(refreshWindows:),
            rect(WIDTH - 100.0, 41.0, 76.0, 25.0),
        );
        refresh.setHidden(accessibility::is_trusted());
        root.addSubview(&refresh);
        let settings_button = self.button(
            "设置…  ⌘,",
            sel!(showSettings:),
            rect(WIDTH - 112.0, 12.0, 90.0, 25.0),
        );
        root.addSubview(&settings_button);

        self.ivars().panel.set(panel).unwrap();
        self.ivars().input.set(input).unwrap();
        self.ivars().list.set(list).unwrap();
        self.ivars().footer.set(footer).unwrap();
        self.ivars().help.set(help).unwrap();
        self.ivars().demo_button.set(demo_button).unwrap();
        self.ivars().refresh_button.set(refresh).unwrap();

        let status_item =
            NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        if let Some(button) = status_item.button(mtm) {
            button.setTitle(ns_string!("▤"));
            button.setToolTip(Some(ns_string!("Winlane · 窗口搜索")));
        }
        let menu = NSMenu::new(mtm);
        for (title, action, key) in [
            ("打开窗口搜索", sel!(showSearch:), ""),
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

    fn register_hotkey(&self) {
        let shortcut = self.ivars().config.borrow().shortcut.clone();
        if let Err(error) = self.replace_hotkey(&shortcut) {
            self.ivars().hotkey_error.replace(Some(error));
        } else {
            self.ivars().hotkey_error.replace(None);
        }
    }

    fn replace_hotkey(&self, shortcut: &Shortcut) -> Result<(), String> {
        let key = shortcut.hotkey()?;
        let old = self.ivars().hotkey.get();
        if shortcut.is_command_tab() {
            if self
                .ivars()
                .command_tab
                .borrow()
                .as_ref()
                .is_some_and(CommandTabTap::is_enabled)
            {
                return Ok(());
            }
            let (tap, receiver) = CommandTabTap::new(self.mtm())?;
            if let Some(old) = old
                && let Some(manager) = self.ivars().hotkey_manager.borrow().as_ref()
            {
                manager
                    .unregister(old)
                    .map_err(|error| format!("无法替换旧快捷键：{error}"))?;
            }
            self.ivars().command_tab.replace(Some(tap));
            self.ivars().command_tab_rx.replace(Some(receiver));
            self.ivars().hotkey.set(None);
            self.ivars().hotkey_press.replace(PressLatch::default());
            return Ok(());
        }
        if old == Some(key) {
            return Ok(());
        }
        let mut manager = self.ivars().hotkey_manager.borrow_mut();
        if manager.is_none() {
            *manager = Some(
                GlobalHotKeyManager::new().map_err(|error| format!("无法初始化快捷键：{error}"))?,
            );
        }
        let manager = manager.as_ref().unwrap();
        manager.register(key).map_err(|error| {
            format!("这个快捷键无法注册，可能已被占用。原设置未改变；可从菜单栏打开。\n{error}")
        })?;
        if let Some(old) = old
            && let Err(error) = manager.unregister(old)
        {
            let rollback = manager.unregister(key);
            return Err(format!(
                "无法替换旧快捷键：{error}。{}",
                if rollback.is_ok() {
                    "新快捷键已撤销，请重试。"
                } else {
                    "请重启应用后重试。"
                }
            ));
        }
        self.ivars().hotkey.set(Some(key));
        self.ivars().hotkey_press.replace(PressLatch::default());
        self.ivars().command_tab.replace(None);
        self.ivars().command_tab_rx.replace(None);
        Ok(())
    }

    fn check_command_tab(&self) {
        let state = self.ivars();
        let tick = state.shortcut_check_tick.get().wrapping_add(1);
        state.shortcut_check_tick.set(tick);
        if !tick.is_multiple_of(25) || !state.config.borrow().shortcut.is_command_tab() {
            return;
        }
        if !accessibility::is_trusted() {
            if state.command_tab.borrow().is_some() {
                state.command_tab.replace(None);
                state.command_tab_rx.replace(None);
                state
                    .hotkey_error
                    .replace(Some("⌘Tab 未启用，请在系统设置中重新允许 Winlane。".into()));
                self.report_shortcut_status();
                self.render();
            }
        } else if state.command_tab.borrow().is_none() {
            self.register_hotkey();
            self.report_shortcut_status();
            self.render();
        } else if !state
            .command_tab
            .borrow()
            .as_ref()
            .is_some_and(CommandTabTap::is_enabled)
        {
            state
                .hotkey_error
                .replace(Some("⌘Tab 监听已暂停，请重新保存快捷键设置。".into()));
            self.report_shortcut_status();
        }
    }

    fn report_shortcut_status(&self) {
        if let Some(settings) = self.ivars().settings.get() {
            if let Some(error) = self.ivars().hotkey_error.borrow().as_deref() {
                settings.report(error, true);
            } else {
                let shortcut = self.ivars().config.borrow().shortcut.display();
                settings.report(&format!("{shortcut} 已启用，设置已保存在本机。"), false);
            }
        }
    }

    fn command_tab_action(&self, action: CommandTabAction) {
        if action == CommandTabAction::Cancel {
            if self.ivars().panel.get().unwrap().isVisible() {
                self.dismiss();
            }
            return;
        }
        let direction = if action == CommandTabAction::Previous {
            -1
        } else {
            1
        };
        if self.ivars().panel.get().unwrap().isVisible() {
            self.ivars().panel.get().unwrap().makeKeyAndOrderFront(None);
            self.focus_search();
            self.move_selection(direction);
        } else {
            if let Some(settings) = self.ivars().settings.get() {
                settings.window.orderOut(None);
            }
            self.show();
            if direction < 0 {
                self.move_selection(-1);
            }
        }
    }

    fn apply_config(&self, candidate: Config) -> Result<(), String> {
        candidate.validate()?;
        self.replace_hotkey(&candidate.shortcut)?;
        settings::save(&candidate)?;
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
            .shortcut_label
            .get()
            .unwrap()
            .setStringValue(&NSString::from_str(&display));
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
        let workspace = NSWorkspace::sharedWorkspace();
        if let Some(front) = workspace.frontmostApplication()
            && front.processIdentifier() != std::process::id() as i32
        {
            self.ivars().previous_pid.set(front.processIdentifier());
        }
        self.ivars()
            .input
            .get()
            .unwrap()
            .setStringValue(ns_string!(""));
        if !self.ivars().demo.get() {
            self.refresh();
        } else {
            self.filter();
        }
        let panel = self.ivars().panel.get().unwrap();
        let mouse = NSEvent::mouseLocation();
        let screens = NSScreen::screens(self.mtm());
        if let Some(screen) = screens.iter().find(|screen| {
            let f = screen.frame();
            mouse.x >= f.origin.x
                && mouse.x <= f.origin.x + f.size.width
                && mouse.y >= f.origin.y
                && mouse.y <= f.origin.y + f.size.height
        }) {
            let frame = screen.visibleFrame();
            panel.setFrameOrigin(NSPoint::new(
                frame.origin.x + (frame.size.width - WIDTH) / 2.0,
                frame.origin.y + (frame.size.height - HEIGHT) * 0.58,
            ));
        } else {
            panel.center();
        }
        panel.makeKeyAndOrderFront(None);
        self.focus_search();
    }

    fn focus_search(&self) {
        if let (Some(panel), Some(input)) = (self.ivars().panel.get(), self.ivars().input.get())
            && panel.isKeyWindow()
            && input.currentEditor().is_none()
        {
            panel.makeFirstResponder(Some(input));
        }
    }

    fn dismiss(&self) {
        self.ivars().panel.get().unwrap().orderOut(None);
        if let Some(previous) = NSRunningApplication::runningApplicationWithProcessIdentifier(
            self.ivars().previous_pid.get(),
        ) {
            self.activate_app(&previous);
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
            self.filter();
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
        self.filter();
    }

    fn filter(&self) {
        self.filter_preserving(None);
    }

    fn filter_preserving(&self, selected_id: Option<u64>) {
        let query = self.ivars().input.get().unwrap().stringValue().to_string();
        let preferred = self
            .ivars()
            .preferences
            .borrow()
            .get(&query.trim().to_lowercase())
            .copied();
        let windows = self.ivars().windows.borrow();
        let scope_pid = if self.ivars().scope.get().unwrap().state() == NSControlStateValueOn {
            Some(if self.ivars().demo.get() {
                -1
            } else {
                self.ivars().previous_pid.get()
            })
        } else {
            None
        };
        let matched = visible_matches(
            &windows,
            &query,
            preferred,
            &self.ivars().config.borrow(),
            scope_pid,
            &self.ivars().recency.borrow(),
            self.ivars().previous_pid.get(),
        );
        let selected = selected_id
            .and_then(|id| matched.iter().position(|&index| windows[index].id == id))
            .unwrap_or(0);
        drop(windows);
        self.ivars().matches.replace(matched);
        self.ivars().selected.set(selected);
        self.render();
    }

    fn move_selection(&self, direction: isize) {
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
        let state = self.ivars();
        let list = state.list.get().unwrap();
        for subview in list.subviews() {
            subview.removeFromSuperview();
        }
        let windows = state.windows.borrow();
        let matched = state.matches.borrow();
        let trusted = accessibility::is_trusted();
        let demo = state.demo.get();
        state.help.get().unwrap().setHidden(trusted || demo);
        state
            .refresh_button
            .get()
            .unwrap()
            .setHidden(trusted || demo);
        state.demo_button.get().unwrap().setHidden(trusted && !demo);
        state.demo_button.get().unwrap().setTitle(if demo {
            ns_string!("返回真实窗口")
        } else {
            ns_string!("查看演示")
        });
        list.setFrameSize(NSSize::new(
            WIDTH - 36.0,
            (matched.len() as f64 * ROW_HEIGHT).max(LIST_HEIGHT),
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
            let selected = position == state.selected.get();
            let row = self.button(
                "",
                sel!(pickWindow:),
                rect(8.0, position as f64 * ROW_HEIGHT + 2.0, WIDTH - 58.0, 60.0),
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
                    let _: () = msg_send![&layer, setCornerRadius: 10.0f64];
                }
            }
            let title = if item.title.trim().is_empty() {
                item.app.as_str()
            } else {
                item.title.as_str()
            };
            let tooltip = format!("{} — {}", item.app, title);
            row.setToolTip(Some(&NSString::from_str(&tooltip)));
            let title_field = label(
                title,
                15.0,
                rect(58.0, 30.0, WIDTH - 220.0, 23.0),
                self.mtm(),
            );
            title_field.setMaximumNumberOfLines(1);
            title_field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
            let subtitle = format!(
                "{}{}",
                item.app,
                if item.minimized {
                    "  ·  已最小化"
                } else {
                    ""
                }
            );
            let subtitle_field = label(
                &subtitle,
                12.0,
                rect(58.0, 9.0, WIDTH - 166.0, 20.0),
                self.mtm(),
            );
            subtitle_field.setTextColor(Some(&NSColor::secondaryLabelColor()));
            if selected {
                title_field.setTextColor(Some(&NSColor::selectedMenuItemTextColor()));
                subtitle_field.setTextColor(Some(&NSColor::selectedMenuItemTextColor()));
            }
            let icon = NSImageView::initWithFrame(
                NSImageView::alloc(self.mtm()),
                rect(12.0, 12.0, 36.0, 36.0),
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
            if position < 9 {
                let key = label(
                    &format!("⌘{}", position + 1),
                    12.0,
                    rect(WIDTH - 116.0, 20.0, 42.0, 22.0),
                    self.mtm(),
                );
                let color = if selected {
                    NSColor::selectedMenuItemTextColor()
                } else {
                    NSColor::tertiaryLabelColor()
                };
                key.setTextColor(Some(&color));
                row.addSubview(&key);
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
        } else if !trusted {
            "需要辅助功能权限；也可以先查看演示。".into()
        } else if state.loading.get() {
            "正在刷新窗口…".into()
        } else {
            format!("{} 项 · ↑↓ 选择  ↵ 切换  Esc 取消", matched.len())
        };
        state
            .footer
            .get()
            .unwrap()
            .setStringValue(&NSString::from_str(&status));
    }

    fn activate_app(&self, app: &NSRunningApplication) -> bool {
        let current = NSRunningApplication::currentApplication();
        NSApplication::sharedApplication(self.mtm()).yieldActivationToApplication(app);
        app.activateFromApplication_options(&current, NSApplicationActivationOptions::empty())
    }

    fn activate_selected(&self) {
        let Some(window) = self.selected_window() else {
            return;
        };
        if self.ivars().demo.get() {
            self.ivars()
                .footer
                .get()
                .unwrap()
                .setStringValue(&NSString::from_str(&format!(
                    "演示选择：{} · {}（未切换真实窗口）",
                    window.app, window.title
                )));
            return;
        }
        let Some(target_app) =
            NSRunningApplication::runningApplicationWithProcessIdentifier(window.pid)
        else {
            self.report_switch_error("应用已退出，请刷新窗口列表。");
            return;
        };
        target_app.unhide();
        if let Err(error) = accessibility::raise_window(window.pid, window.id) {
            self.report_switch_error(&error);
            return;
        }
        if !self.activate_app(&target_app) {
            self.report_switch_error("系统未接受切换请求，请重试或检查辅助功能权限。");
            return;
        }
        let mut recent = self.ivars().recency.borrow_mut();
        recent.retain(|id| *id != window.id);
        recent.insert(0, window.id);
        recent.truncate(128);
        let query = self
            .ivars()
            .input
            .get()
            .unwrap()
            .stringValue()
            .to_string()
            .trim()
            .to_lowercase();
        if !query.is_empty() {
            let mut preferences = self.ivars().preferences.borrow_mut();
            if preferences.len() > 128 {
                preferences.clear();
            }
            preferences.insert(query, window.id);
        }
        self.ivars().panel.get().unwrap().orderOut(None);
    }

    fn report_switch_error(&self, text: &str) {
        self.ivars()
            .footer
            .get()
            .unwrap()
            .setStringValue(&NSString::from_str(text));
        self.ivars()
            .footer
            .get()
            .unwrap()
            .setToolTip(Some(&NSString::from_str(text)));
    }

    fn minimize_selected(&self) {
        if !self.ivars().panel.get().unwrap().isVisible() {
            return;
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
        if !self.ivars().panel.get().unwrap().isVisible() {
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
