use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{MainThreadOnly, sel};
use objc2_app_kit::*;
use objc2_foundation::{
    MainThreadMarker, NSPoint, NSRect, NSSize, NSString, NSUserDefaults, ns_string,
};
use objc2_service_management::{SMAppService, SMAppServiceStatus};
use winlane::aliases::Aliases;
use winlane::config::{Appearance, Config, KEYS, Shortcut, SortOrder, key_label, parse_excluded};

pub fn load_aliases() -> Result<Aliases, String> {
    NSUserDefaults::standardUserDefaults()
        .stringForKey(ns_string!("WinlaneAliasesV1"))
        .map(|value| Aliases::from_json(&value.to_string()))
        .unwrap_or_else(|| Ok(Aliases::default()))
}

pub fn save_aliases(aliases: &Aliases) {
    let json = NSString::from_str(&aliases.to_json());
    // SAFETY: The value is a property-list string, stored separately from shortcut settings.
    unsafe {
        NSUserDefaults::standardUserDefaults()
            .setObject_forKey(Some(&json), ns_string!("WinlaneAliasesV1"));
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

pub fn apply_appearance(config: &Config, mtm: MainThreadMarker) {
    let appearance = match config.appearance {
        Appearance::System => None,
        // SAFETY: These immutable AppKit constants are available on all supported systems.
        Appearance::Light => NSAppearance::appearanceNamed(unsafe { NSAppearanceNameAqua }),
        Appearance::Dark => NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }),
    };
    NSApplication::sharedApplication(mtm).setAppearance(appearance.as_deref());
}

struct ShortcutControls {
    modifiers: [Retained<NSButton>; 4],
    key: Retained<NSPopUpButton>,
    keys: Vec<&'static str>,
}

impl ShortcutControls {
    fn new(view: &NSView, title: &str, y: f64, switching: bool, mtm: MainThreadMarker) -> Self {
        view.addSubview(&label(title, 14.0, rect(30.0, y, 500.0, 24.0), mtm));
        let modifiers =
            ["⌃ Control", "⌥ Option", "⇧ Shift", "⌘ Command"].map(|title| checkbox(title, mtm));
        for (i, control) in modifiers.iter().enumerate() {
            control.setFrame(rect(30.0 + i as f64 * 110.0, y - 32.0, 108.0, 25.0));
            view.addSubview(control);
        }
        let keys: Vec<_> = KEYS
            .iter()
            .copied()
            .filter(|key| !switching || *key != "Space")
            .collect();
        let key = popup(
            &keys.iter().map(|key| key_label(key)).collect::<Vec<_>>(),
            rect(480.0, y - 34.0, 150.0, 28.0),
            mtm,
        );
        view.addSubview(&key);
        Self {
            modifiers,
            key,
            keys,
        }
    }

    fn fill(&self, shortcut: &Shortcut) {
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

    fn read(&self) -> Result<Shortcut, String> {
        let [control, option, shift, command] =
            std::array::from_fn(|i| self.modifiers[i].state() == NSControlStateValueOn);
        let key = self
            .keys
            .get(self.key.indexOfSelectedItem() as usize)
            .ok_or("请选择快捷键。")?;
        Ok(Shortcut {
            control,
            option,
            shift,
            command,
            key: (*key).into(),
        })
    }
}

pub struct SettingsWindow {
    pub window: Retained<NSWindow>,
    search_shortcut: ShortcutControls,
    switch_shortcut: ShortcutControls,
    sort: Retained<NSPopUpButton>,
    appearance: Retained<NSPopUpButton>,
    minimized: Retained<NSButton>,
    excluded: Retained<NSTextField>,
    login: Retained<NSButton>,
    login_status: Retained<NSTextField>,
    message: Retained<NSTextField>,
}

impl SettingsWindow {
    pub fn new(target: &AnyObject, mtm: MainThreadMarker) -> Self {
        // SAFETY: The window is created on the main thread and retained across closes below.
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                rect(0.0, 0.0, 660.0, 740.0),
                NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        // SAFETY: SettingsWindow retains this reusable window after close.
        unsafe { window.setReleasedWhenClosed(false) };
        window.setTitle(ns_string!("Winlane 设置"));
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 660.0, 740.0));
        window.setContentView(Some(&view));
        view.addSubview(&label(
            "让窗口切换顺手一点",
            23.0,
            rect(30.0, 684.0, 520.0, 33.0),
            mtm,
        ));
        view.addSubview(&hint(
            "设置保存在本机；窗口标题与搜索历史不会保存。",
            rect(30.0, 655.0, 520.0, 25.0),
            mtm,
        ));
        let search_shortcut = ShortcutControls::new(&view, "搜索模式快捷键", 616.0, false, mtm);
        view.addSubview(&hint(
            "保持面板，Enter 确认；空搜索时 Space 切换模式。",
            rect(30.0, 551.0, 600.0, 25.0),
            mtm,
        ));
        let switch_shortcut = ShortcutControls::new(&view, "切换模式快捷键", 516.0, true, mtm);
        view.addSubview(&hint(
            "重复按键选择，松开主修饰键确认；Space 进入搜索。",
            rect(30.0, 451.0, 600.0, 25.0),
            mtm,
        ));

        view.addSubview(&label(
            "窗口排序",
            14.0,
            rect(30.0, 404.0, 140.0, 25.0),
            mtm,
        ));
        let sort = popup(
            &["最近在 Winlane 中切换", "应用名称", "窗口标题"],
            rect(180.0, 401.0, 370.0, 28.0),
            mtm,
        );
        view.addSubview(&sort);
        view.addSubview(&label("外观", 14.0, rect(30.0, 362.0, 140.0, 25.0), mtm));
        let appearance = popup(
            &["跟随系统", "浅色", "深色"],
            rect(180.0, 359.0, 370.0, 28.0),
            mtm,
        );
        view.addSubview(&appearance);
        let minimized = checkbox("在列表中显示最小化窗口", mtm);
        minimized.setFrame(rect(30.0, 321.0, 500.0, 26.0));
        view.addSubview(&minimized);
        view.addSubview(&hint(
            "已授权后，启动时仅显示菜单栏图标。",
            rect(30.0, 287.0, 500.0, 26.0),
            mtm,
        ));

        view.addSubview(&label(
            "排除这些应用",
            14.0,
            rect(30.0, 246.0, 500.0, 25.0),
            mtm,
        ));
        let excluded =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(30.0, 212.0, 520.0, 27.0));
        excluded.setPlaceholderString(Some(ns_string!("例如：Finder, Terminal")));
        view.addSubview(&excluded);
        view.addSubview(&hint(
            "填写列表里显示的完整应用名，用逗号分隔。",
            rect(30.0, 185.0, 520.0, 24.0),
            mtm,
        ));

        let login = checkbox("登录时自动启动", mtm);
        login.setFrame(rect(30.0, 136.0, 270.0, 27.0));
        set_action(&login, target, sel!(toggleLogin:));
        view.addSubview(&login);
        view.addSubview(&button(
            "管理登录项…",
            target,
            sel!(manageLogin:),
            rect(370.0, 136.0, 180.0, 28.0),
            mtm,
        ));
        let login_status = hint("", rect(30.0, 93.0, 520.0, 38.0), mtm);
        login_status.setMaximumNumberOfLines(2);
        view.addSubview(&login_status);
        let message = hint("", rect(30.0, 47.0, 600.0, 38.0), mtm);
        message.setMaximumNumberOfLines(2);
        view.addSubview(&message);
        view.addSubview(&button(
            "恢复默认设置",
            target,
            sel!(resetSettings:),
            rect(30.0, 9.0, 160.0, 30.0),
            mtm,
        ));
        view.addSubview(&button(
            "保存设置",
            target,
            sel!(saveSettings:),
            rect(490.0, 9.0, 140.0, 30.0),
            mtm,
        ));

        Self {
            window,
            search_shortcut,
            switch_shortcut,
            sort,
            appearance,
            minimized,
            excluded,
            login,
            login_status,
            message,
        }
    }

    pub fn fill(&self, config: &Config) {
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
            include_minimized: self.minimized.state() == NSControlStateValueOn,
            excluded_apps: parse_excluded(&self.excluded.stringValue().to_string()),
        };
        config.validate()?;
        Ok(config)
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
            "快捷键和列表设置在保存后生效；登录启动开关立即生效。",
            false,
        );
        self.window.center();
        self.window.makeKeyAndOrderFront(None);
    }

    pub fn update_login_status(&self) {
        // SAFETY: macOS 14+ supports the main application's login service. Status is read-only.
        let status = unsafe { SMAppService::mainAppService().status() };
        let (enabled, text) = match status {
            SMAppServiceStatus::Enabled => (true, "已启用；下次登录时启动 Winlane。"),
            SMAppServiceStatus::RequiresApproval => {
                (true, "等待系统允许：请在“管理登录项”中开启。")
            }
            SMAppServiceStatus::NotFound => (false, "尚无可用登录项。启用时会检查应用位置与签名。"),
            _ => (false, "未启用。"),
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
                &format!("无法更新登录启动：{}", error.localizedDescription()),
                true,
            );
        } else {
            self.report("登录启动状态已更新。", false);
        }
    }
}

pub fn manage_login() {
    // SAFETY: Opens the documented settings pane in response to the user's button click.
    unsafe { SMAppService::openSystemSettingsLoginItems() };
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}
fn label(text: &str, size: f64, frame: NSRect, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let value = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    value.setFont(Some(&NSFont::systemFontOfSize(size)));
    value.setFrame(frame);
    value
}
fn hint(text: &str, frame: NSRect, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let value = label(text, 12.0, frame, mtm);
    value.setTextColor(Some(&NSColor::secondaryLabelColor()));
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
fn button(
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
