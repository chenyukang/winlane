use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{MainThreadOnly, sel};
use objc2_app_kit::*;
use objc2_foundation::{
    MainThreadMarker, NSLocale, NSPoint, NSRect, NSSize, NSString, NSUserDefaults, ns_string,
};
use objc2_service_management::{SMAppService, SMAppServiceStatus};
use std::cell::RefCell;
use winlane::aliases::Aliases;
use winlane::config::{
    AppShortcut, Appearance, Config, KEYS, Shortcut, SortOrder, key_label, parse_excluded,
};
use winlane::i18n::{self, Language};
use winlane::{tr, trf};

pub fn load_aliases() -> Result<Aliases, String> {
    let defaults = NSUserDefaults::standardUserDefaults();
    defaults
        .stringForKey(ns_string!("WinlaneAliasesV2"))
        .or_else(|| defaults.stringForKey(ns_string!("WinlaneAliasesV1")))
        .map(|value| Aliases::from_json(&value.to_string()))
        .unwrap_or_else(|| Ok(Aliases::default()))
}

pub fn save_aliases(aliases: &Aliases) {
    let json = NSString::from_str(&aliases.to_json());
    // SAFETY: The value is a property-list string, stored separately from shortcut settings.
    unsafe {
        NSUserDefaults::standardUserDefaults()
            .setObject_forKey(Some(&json), ns_string!("WinlaneAliasesV2"));
    }
}

fn storage_key() -> &'static NSString {
    ns_string!("WindowlanePreferencesV1")
}

pub fn load() -> Result<Config, String> {
    NSUserDefaults::standardUserDefaults()
        .stringForKey(storage_key())
        .map(|value| Config::from_json(&value.to_string()))
        .unwrap_or_else(|| Ok(Config::default()))
}

pub fn save(config: &Config) -> Result<(), String> {
    let json = NSString::from_str(&config.to_json()?);
    // SAFETY: NSString is an accepted property-list value. One key stores the whole validated configuration.
    unsafe { NSUserDefaults::standardUserDefaults().setObject_forKey(Some(&json), storage_key()) };
    Ok(())
}

pub fn apply_language(language: Language) -> bool {
    let languages: Vec<_> = NSLocale::preferredLanguages()
        .iter()
        .map(|value| value.to_string())
        .collect();
    i18n::set_locale(language.resolve(languages.iter().map(String::as_str)))
}

pub fn apply_appearance(config: &Config, mtm: MainThreadMarker) {
    let appearance = match config.appearance {
        Appearance::System => None,
        // SAFETY: These immutable AppKit constants are available on all supported systems.
        Appearance::Light => NSAppearance::appearanceNamed(unsafe { NSAppearanceNameAqua }),
        Appearance::Dark => NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }),
    };
    NSApplication::sharedApplication(mtm).setAppearance(appearance.as_deref());
}

pub(crate) struct ShortcutControls {
    modifiers: [Retained<NSButton>; 4],
    key: Retained<NSPopUpButton>,
    keys: Vec<&'static str>,
}

impl ShortcutControls {
    fn new(view: &NSView, title: &str, y: f64, switching: bool, mtm: MainThreadMarker) -> Self {
        view.addSubview(&label(title, 14.0, rect(30.0, y, 500.0, 24.0), mtm));
        Self::at(view, y - 32.0, switching, mtm)
    }

    pub(crate) fn at(view: &NSView, y: f64, switching: bool, mtm: MainThreadMarker) -> Self {
        let modifiers =
            ["⌃ Control", "⌥ Option", "⇧ Shift", "⌘ Command"].map(|title| checkbox(title, mtm));
        for (i, control) in modifiers.iter().enumerate() {
            control.setFrame(rect(30.0 + i as f64 * 110.0, y, 108.0, 25.0));
            view.addSubview(control);
        }
        let keys: Vec<_> = KEYS
            .iter()
            .copied()
            .filter(|key| !switching || *key != "Space")
            .collect();
        let key = popup(
            &keys.iter().map(|key| key_label(key)).collect::<Vec<_>>(),
            rect(480.0, y - 2.0, 150.0, 28.0),
            mtm,
        );
        view.addSubview(&key);
        Self {
            modifiers,
            key,
            keys,
        }
    }

    pub(crate) fn fill(&self, shortcut: &Shortcut) {
        for (control, enabled) in self.modifiers.iter().zip([
            shortcut.control,
            shortcut.option,
            shortcut.shift,
            shortcut.command,
        ]) {
            control.setState(if enabled {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
        self.key.selectItemAtIndex(
            self.keys
                .iter()
                .position(|key| *key == shortcut.key)
                .unwrap_or(0) as isize,
        );
    }

    pub(crate) fn read(&self) -> Result<Shortcut, String> {
        let [control, option, shift, command] =
            std::array::from_fn(|i| self.modifiers[i].state() == NSControlStateValueOn);
        let key = self
            .keys
            .get(self.key.indexOfSelectedItem() as usize)
            .ok_or(tr!("请选择快捷键。", "Choose a shortcut."))?;
        Ok(Shortcut {
            control,
            option,
            shift,
            command,
            key: (*key).into(),
        })
    }

    pub(crate) fn copy_from(&self, other: &Self) {
        for (control, previous) in self.modifiers.iter().zip(&other.modifiers) {
            control.setState(previous.state());
        }
        self.key.selectItemAtIndex(other.key.indexOfSelectedItem());
    }
}

pub struct SettingsWindow {
    pub window: Retained<NSWindow>,
    search_shortcut: ShortcutControls,
    switch_shortcut: ShortcutControls,
    tabs: Retained<NSTabView>,
    language: Retained<NSPopUpButton>,
    sort: Retained<NSPopUpButton>,
    appearance: Retained<NSPopUpButton>,
    minimized: Retained<NSButton>,
    excluded: Retained<NSTextField>,
    login: Retained<NSButton>,
    login_status: Retained<NSTextField>,
    message: Retained<NSTextField>,
    app_shortcuts: RefCell<Vec<AppShortcut>>,
}

impl SettingsWindow {
    pub fn new(target: &AnyObject, mtm: MainThreadMarker) -> Self {
        // SAFETY: The window is created on the main thread and retained across closes below.
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                rect(0.0, 0.0, 720.0, 620.0),
                NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        // SAFETY: SettingsWindow retains this reusable window after close.
        unsafe { window.setReleasedWhenClosed(false) };
        window.setTitle(&NSString::from_str(tr!("Winlane 设置", "Winlane Settings")));
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 720.0, 620.0));
        window.setContentView(Some(&view));
        view.addSubview(&label(
            tr!("Winlane 设置", "Winlane Settings"),
            23.0,
            rect(28.0, 558.0, 650.0, 33.0),
            mtm,
        ));
        view.addSubview(&hint(
            tr!(
                "设置保存在本机；窗口标题与搜索历史不会保存。",
                "Settings stay on this Mac. Window titles and search history are not saved."
            ),
            rect(28.0, 530.0, 660.0, 25.0),
            mtm,
        ));
        let tabs = NSTabView::initWithFrame(NSTabView::alloc(mtm), rect(20.0, 112.0, 680.0, 404.0));
        tabs.setTabViewType(NSTabViewType::TopTabsBezelBorder);
        view.addSubview(&tabs);
        let shortcuts = settings_tab(&tabs, tr!("快捷键", "Shortcuts"), mtm);
        let appearance_tab = settings_tab(&tabs, tr!("外观与语言", "Appearance & Language"), mtm);
        let windows = settings_tab(&tabs, tr!("窗口列表", "Window List"), mtm);
        let startup = settings_tab(&tabs, tr!("启动", "Startup"), mtm);

        let search_shortcut = ShortcutControls::new(
            &shortcuts,
            tr!("搜索模式", "Search mode"),
            315.0,
            false,
            mtm,
        );
        shortcuts.addSubview(&hint(
            tr!(
                "保持面板，Enter 确认；空搜索时 Space 切换模式。",
                "Keep the panel open; Enter selects. Space switches modes when search is empty."
            ),
            rect(30.0, 244.0, 600.0, 32.0),
            mtm,
        ));
        let switch_shortcut =
            ShortcutControls::new(&shortcuts, tr!("切换模式", "Switch mode"), 207.0, true, mtm);
        shortcuts.addSubview(&hint(
            tr!(
                "重复按键选择，松开主修饰键确认；Space 进入搜索。",
                "Press repeatedly to select; release the modifier to switch. Space opens search."
            ),
            rect(30.0, 136.0, 600.0, 32.0),
            mtm,
        ));
        let divider = NSBox::initWithFrame(NSBox::alloc(mtm), rect(30.0, 109.0, 600.0, 1.0));
        divider.setBoxType(NSBoxType::Separator);
        shortcuts.addSubview(&divider);
        shortcuts.addSubview(&label(
            tr!("应用快捷键", "App shortcuts"),
            14.0,
            rect(30.0, 68.0, 410.0, 24.0),
            mtm,
        ));
        shortcuts.addSubview(&hint(
            tr!(
                "为常用应用绑定固定快捷键，未运行时自动启动。",
                "Assign a shortcut to any app. Launch it if it is not running."
            ),
            rect(30.0, 26.0, 410.0, 38.0),
            mtm,
        ));
        shortcuts.addSubview(&button(
            tr!("配置应用快捷键…", "App Shortcuts…"),
            target,
            sel!(showAppShortcuts:),
            rect(455.0, 61.0, 175.0, 32.0),
            mtm,
        ));

        appearance_tab.addSubview(&label(
            tr!("界面语言", "Language"),
            14.0,
            rect(30.0, 296.0, 215.0, 25.0),
            mtm,
        ));
        let language = popup(
            &[tr!("跟随系统", "System"), "中文", "English"],
            rect(260.0, 293.0, 370.0, 28.0),
            mtm,
        );
        appearance_tab.addSubview(&language);
        appearance_tab.addSubview(&hint(
            tr!(
                "跟随 macOS 的首选语言；保存后立即生效。",
                "Use your preferred macOS language. Changes take effect when saved."
            ),
            rect(30.0, 250.0, 600.0, 32.0),
            mtm,
        ));
        appearance_tab.addSubview(&label(
            tr!("外观", "Appearance"),
            14.0,
            rect(30.0, 191.0, 215.0, 25.0),
            mtm,
        ));
        let appearance = popup(
            &[
                tr!("跟随系统", "System"),
                tr!("浅色", "Light"),
                tr!("深色", "Dark"),
            ],
            rect(260.0, 188.0, 370.0, 28.0),
            mtm,
        );
        appearance_tab.addSubview(&appearance);
        appearance_tab.addSubview(&hint(
            tr!(
                "用于搜索、切换面板和设置窗口。",
                "Applies to the search panel, switch panel, and settings."
            ),
            rect(30.0, 145.0, 600.0, 32.0),
            mtm,
        ));

        windows.addSubview(&label(
            tr!("窗口排序", "Sort windows"),
            14.0,
            rect(30.0, 296.0, 215.0, 25.0),
            mtm,
        ));
        let sort = popup(
            &[
                tr!("最近在 Winlane 中切换", "Recently switched in Winlane"),
                tr!("应用名称", "Application name"),
                tr!("窗口标题", "Window title"),
            ],
            rect(260.0, 293.0, 370.0, 28.0),
            mtm,
        );
        windows.addSubview(&sort);
        let minimized = checkbox(
            tr!("在列表中显示最小化窗口", "Include minimized windows"),
            mtm,
        );
        minimized.setFrame(rect(30.0, 237.0, 600.0, 26.0));
        windows.addSubview(&minimized);
        windows.addSubview(&label(
            tr!("排除这些应用", "Excluded apps"),
            14.0,
            rect(30.0, 173.0, 600.0, 25.0),
            mtm,
        ));
        let excluded =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(30.0, 134.0, 600.0, 28.0));
        excluded.setPlaceholderString(Some(&NSString::from_str(tr!(
            "例如：Finder, Terminal",
            "For example: Finder, Terminal"
        ))));
        windows.addSubview(&excluded);
        windows.addSubview(&hint(
            tr!(
                "填写列表里显示的完整应用名，用逗号分隔。",
                "Enter app names exactly as shown in the list, separated by commas."
            ),
            rect(30.0, 85.0, 600.0, 38.0),
            mtm,
        ));

        let login = checkbox(tr!("登录时自动启动", "Launch at login"), mtm);
        login.setFrame(rect(30.0, 293.0, 330.0, 27.0));
        set_action(&login, target, sel!(toggleLogin:));
        startup.addSubview(&login);
        startup.addSubview(&button(
            tr!("管理登录项…", "Manage Login Items…"),
            target,
            sel!(manageLogin:),
            rect(430.0, 292.0, 200.0, 28.0),
            mtm,
        ));
        let login_status = hint("", rect(30.0, 237.0, 600.0, 43.0), mtm);
        login_status.setMaximumNumberOfLines(2);
        startup.addSubview(&login_status);
        startup.addSubview(&hint(
            tr!(
                "已授权后，启动时仅显示菜单栏图标。",
                "Once authorized, Winlane starts quietly in the menu bar."
            ),
            rect(30.0, 177.0, 600.0, 38.0),
            mtm,
        ));
        startup.addSubview(&hint(
            tr!(
                "登录启动开关立即生效。",
                "The launch-at-login toggle takes effect immediately."
            ),
            rect(30.0, 131.0, 600.0, 32.0),
            mtm,
        ));

        let message = hint("", rect(28.0, 62.0, 664.0, 38.0), mtm);
        message.setMaximumNumberOfLines(2);
        view.addSubview(&message);
        view.addSubview(&button(
            tr!("恢复默认设置", "Restore Defaults"),
            target,
            sel!(resetSettings:),
            rect(28.0, 20.0, 190.0, 30.0),
            mtm,
        ));
        view.addSubview(&button(
            tr!("保存设置", "Save Settings"),
            target,
            sel!(saveSettings:),
            rect(542.0, 20.0, 150.0, 30.0),
            mtm,
        ));

        Self {
            window,
            tabs,
            language,
            search_shortcut,
            switch_shortcut,
            sort,
            appearance,
            minimized,
            excluded,
            login,
            login_status,
            message,
            app_shortcuts: RefCell::default(),
        }
    }

    pub fn fill(&self, config: &Config) {
        self.set_app_shortcuts(&config.app_shortcuts);
        self.search_shortcut.fill(&config.shortcut);
        self.switch_shortcut.fill(&config.switch_shortcut);
        self.sort.selectItemAtIndex(match config.sort {
            SortOrder::Recent => 0,
            SortOrder::Application => 1,
            SortOrder::Title => 2,
        });
        self.appearance.selectItemAtIndex(match config.appearance {
            Appearance::System => 0,
            Appearance::Light => 1,
            Appearance::Dark => 2,
        });
        self.language.selectItemAtIndex(match config.language {
            Language::System => 0,
            Language::Chinese => 1,
            Language::English => 2,
        });
        self.minimized.setState(if config.include_minimized {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        self.excluded
            .setStringValue(&NSString::from_str(&config.excluded_apps.join(", ")));
        self.update_login_status();
    }

    pub fn candidate(&self) -> Result<Config, String> {
        let config = Config {
            shortcut: self.search_shortcut.read()?,
            switch_shortcut: self.switch_shortcut.read()?,
            app_shortcuts: self.app_shortcuts.borrow().clone(),
            sort: match self.sort.indexOfSelectedItem() {
                1 => SortOrder::Application,
                2 => SortOrder::Title,
                _ => SortOrder::Recent,
            },
            appearance: match self.appearance.indexOfSelectedItem() {
                1 => Appearance::Light,
                2 => Appearance::Dark,
                _ => Appearance::System,
            },
            language: match self.language.indexOfSelectedItem() {
                1 => Language::Chinese,
                2 => Language::English,
                _ => Language::System,
            },
            include_minimized: self.minimized.state() == NSControlStateValueOn,
            excluded_apps: parse_excluded(&self.excluded.stringValue().to_string()),
        };
        config.validate()?;
        Ok(config)
    }

    pub fn selected_tab(&self) -> isize {
        self.tabs
            .selectedTabViewItem()
            .map_or(0, |item| self.tabs.indexOfTabViewItem(&item))
    }

    pub fn select_tab(&self, index: isize) {
        if (0..4).contains(&index) {
            self.tabs.selectTabViewItemAtIndex(index);
        }
    }

    pub fn set_app_shortcuts(&self, shortcuts: &[AppShortcut]) {
        self.app_shortcuts.replace(shortcuts.to_vec());
    }

    pub fn report(&self, message: &str, error: bool) {
        let color = if error {
            NSColor::systemRedColor()
        } else {
            NSColor::secondaryLabelColor()
        };
        self.message.setTextColor(Some(&color));
        self.message.setStringValue(&NSString::from_str(message));
    }

    pub fn show(&self, config: &Config) {
        self.fill(config);
        self.report(
            tr!(
                "快捷键和列表设置在保存后生效；登录启动开关立即生效。",
                "Save to apply changes. The launch-at-login toggle takes effect immediately."
            ),
            false,
        );
        self.window.center();
        self.window.makeKeyAndOrderFront(None);
    }

    pub fn update_login_status(&self) {
        // SAFETY: macOS 14+ supports the main application's login service. Status is read-only.
        let status = unsafe { SMAppService::mainAppService().status() };
        let (enabled, text) = match status {
            SMAppServiceStatus::Enabled => (
                true,
                tr!(
                    "已启用；下次登录时启动 Winlane。",
                    "Enabled. Winlane will start when you next log in."
                ),
            ),
            SMAppServiceStatus::RequiresApproval => (
                true,
                tr!(
                    "等待系统允许：请在“管理登录项”中开启。",
                    "Approval needed. Enable Winlane in Manage Login Items."
                ),
            ),
            SMAppServiceStatus::NotFound => (
                false,
                tr!(
                    "尚无可用登录项。启用时会检查应用位置与签名。",
                    "No login item yet. Enabling checks the app's location and signature."
                ),
            ),
            _ => (false, tr!("未启用。", "Disabled.")),
        };
        self.login.setState(if enabled {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        self.login_status.setStringValue(&NSString::from_str(text));
    }

    pub fn toggle_login(&self) {
        // SAFETY: Invoked only by the user's explicit login-item switch action, on the main thread.
        let result = unsafe {
            let service = SMAppService::mainAppService();
            if self.login.state() == NSControlStateValueOn {
                if service.status() == SMAppServiceStatus::Enabled
                    || service.status() == SMAppServiceStatus::RequiresApproval
                {
                    Ok(())
                } else {
                    service.registerAndReturnError()
                }
            } else if service.status() == SMAppServiceStatus::NotRegistered {
                Ok(())
            } else {
                service.unregisterAndReturnError()
            }
        };
        self.update_login_status();
        if let Err(error) = result {
            self.report(
                &trf!(
                    "无法更新登录启动：{}",
                    "Could not update launch at login: {}",
                    error.localizedDescription()
                ),
                true,
            );
        } else {
            self.report(
                tr!("登录启动状态已更新。", "Launch-at-login setting updated."),
                false,
            );
        }
    }
}

fn settings_tab(tabs: &NSTabView, title: &str, mtm: MainThreadMarker) -> Retained<NSView> {
    let item = NSTabViewItem::new();
    item.setLabel(&NSString::from_str(title));
    let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 660.0, 360.0));
    item.setView(Some(&view));
    tabs.addTabViewItem(&item);
    view
}

pub fn manage_login() {
    // SAFETY: Opens the documented settings pane in response to the user's button click.
    unsafe { SMAppService::openSystemSettingsLoginItems() };
}

pub(crate) fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}
pub(crate) fn label(
    text: &str,
    size: f64,
    frame: NSRect,
    mtm: MainThreadMarker,
) -> Retained<NSTextField> {
    let value = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    value.setFont(Some(&NSFont::systemFontOfSize(size)));
    value.setFrame(frame);
    value
}
pub(crate) fn hint(text: &str, frame: NSRect, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let value = label(text, 12.0, frame, mtm);
    value.setTextColor(Some(&NSColor::secondaryLabelColor()));
    value.setMaximumNumberOfLines(2);
    value
}
fn checkbox(title: &str, mtm: MainThreadMarker) -> Retained<NSButton> {
    let control = NSButton::new(mtm);
    control.setButtonType(NSButtonType::Switch);
    control.setTitle(&NSString::from_str(title));
    control
}
fn popup(titles: &[&str], frame: NSRect, mtm: MainThreadMarker) -> Retained<NSPopUpButton> {
    let control = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), frame, false);
    for title in titles {
        control.addItemWithTitle(&NSString::from_str(title));
    }
    control
}
fn set_action(button: &NSButton, target: &AnyObject, action: Sel) {
    // SAFETY: The application delegate outlives the controls and implements each selector.
    unsafe {
        button.setTarget(Some(target));
        button.setAction(Some(action));
    }
}
pub(crate) fn button(
    title: &str,
    target: &AnyObject,
    action: Sel,
    frame: NSRect,
    mtm: MainThreadMarker,
) -> Retained<NSButton> {
    let button = NSButton::initWithFrame(NSButton::alloc(mtm), frame);
    button.setTitle(&NSString::from_str(title));
    button.setBezelStyle(NSBezelStyle::Push);
    set_action(&button, target, action);
    button
}
