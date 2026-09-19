mod command_shortcuts;
mod layout;
mod navigation;

use super::controls::*;
use super::shortcut::ShortcutControls;
use navigation::SettingsNavigationButton;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};
use objc2_service_management::{SMAppService, SMAppServiceStatus};
use std::cell::RefCell;
use winlane::core::config::{
    AliasRule, AppShortcut, Appearance, Config, DisplayDensity, Shortcut, SortOrder, parse_excluded,
};
use winlane::core::i18n::Language;
use winlane::core::input_method::InputMethod;
use winlane::{tr, trf};

struct SearchShortcutRow {
    view: Retained<NSView>,
    shortcut: ShortcutControls,
    remove: Retained<NSButton>,
}

pub struct SettingsWindow {
    pub window: Retained<NSWindow>,
    search_shortcut: ShortcutControls,
    search_rows: RefCell<Vec<SearchShortcutRow>>,
    shortcuts_document: Retained<NSView>,
    shortcuts_scroll: Retained<NSScrollView>,
    search_header: Retained<NSView>,
    add_search: Retained<NSButton>,
    switch_shortcut: ShortcutControls,
    tabs: Retained<NSTabView>,
    navigation: Vec<Retained<SettingsNavigationButton>>,
    page_title: Retained<NSTextField>,
    page_description: Retained<NSTextField>,
    search_card: Retained<NSView>,
    switch_card: Retained<NSView>,
    app_shortcuts_card: Retained<NSView>,
    command_shortcuts: command_shortcuts::CommandShortcutControls,
    snippets_tab: Retained<NSView>,
    quicklinks_tab: Retained<NSView>,
    clipboard: crate::macos::ui::clipboard_settings::ClipboardControls,
    language: Retained<NSPopUpButton>,
    input_method: Retained<NSPopUpButton>,
    sort: Retained<NSPopUpButton>,
    appearance: Retained<NSPopUpButton>,
    density: Retained<NSPopUpButton>,
    opacity_slider: Retained<NSSlider>,
    opacity_input: Retained<NSTextField>,
    switch_delay: Retained<NSTextField>,
    opacity_preview: crate::macos::ui::material::PanelBackdrop,
    usage_hints: Retained<NSButton>,
    minimized: Retained<NSButton>,
    excluded: Retained<NSTextField>,
    login: Retained<NSButton>,
    login_status: Retained<NSTextField>,
    automatic_updates: Retained<NSButton>,
    check_updates: Retained<NSButton>,
    update_status: Retained<NSTextField>,
    message: Retained<NSTextField>,
    app_shortcuts: RefCell<Vec<AppShortcut>>,
    alias_rules: RefCell<Vec<AliasRule>>,
    snippets: RefCell<Vec<winlane::features::snippets::Snippet>>,
    quicklinks: RefCell<Vec<winlane::features::quicklinks::Quicklink>>,
}

impl SettingsWindow {
    pub fn fill(&self, config: &Config) {
        self.set_snippets(&config.snippets);
        self.set_quicklinks(&config.quicklinks);
        self.fill_clipboard(&config.clipboard);
        self.input_method
            .selectItemAtIndex(match config.input_method {
                InputMethod::Current => 0,
                InputMethod::English => 1,
                InputMethod::Chinese => 2,
                InputMethod::LastUsed => 3,
            });
        self.set_opacity(config.background_opacity);
        self.usage_hints.setState(if config.show_usage_hints {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        self.switch_delay
            .setStringValue(&NSString::from_str(&config.switch_delay_ms.to_string()));
        self.set_app_shortcuts(&config.app_shortcuts);
        self.set_command_shortcuts(&config.command_shortcuts);
        self.set_alias_rules(&config.alias_rules);
        self.search_shortcut.fill(&config.shortcut);
        for row in self.search_rows.borrow_mut().drain(..) {
            row.view.removeFromSuperview();
        }
        for shortcut in &config.additional_search_shortcuts {
            self.append_search_row(shortcut);
        }
        self.layout_search_shortcuts();
        self.search_header
            .scrollRectToVisible(self.search_header.bounds());
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
        self.density
            .selectItemAtIndex(match config.display_density {
                DisplayDensity::Compact => 0,
                DisplayDensity::Normal => 1,
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

    fn append_search_row(&self, value: &Shortcut) {
        let mtm = self.window.mtm();
        let target = self.search_shortcut.key.target().unwrap();
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 660.0, 40.0));
        let shortcut = ShortcutControls::at(&view, 8.0, false, mtm);
        shortcut.key.setFrame(rect(480.0, 6.0, 115.0, 28.0));
        shortcut.fill(value);
        shortcut.on_change(&target, sel!(settingsChanged:));
        let remove = button(
            "−",
            &target,
            sel!(removeSearchShortcut:),
            rect(605.0, 6.0, 32.0, 28.0),
            mtm,
        );
        remove.setToolTip(Some(&NSString::from_str(tr!(
            "移除搜索快捷键",
            "Remove search shortcut"
        ))));
        remove.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "移除搜索快捷键",
            "Remove search shortcut"
        ))));
        view.addSubview(&remove);
        self.search_card.addSubview(&view);
        self.search_rows.borrow_mut().push(SearchShortcutRow {
            view,
            shortcut,
            remove,
        });
    }

    fn layout_search_shortcuts(&self) {
        let rows = self.search_rows.borrow();
        let search_height = 104.0 + rows.len() as f64 * 44.0;
        let height = self
            .shortcuts_scroll
            .contentSize()
            .height
            .max(search_height + 224.0 + self.command_shortcuts.view.frame().size.height);
        self.shortcuts_document
            .setFrameSize(NSSize::new(740.0, height));
        self.search_card
            .setFrame(rect(0.0, height - search_height, 740.0, search_height));
        self.search_header
            .setFrameOrigin(NSPoint::new(40.0, search_height - 104.0));
        for (index, row) in rows.iter().enumerate() {
            row.view.setFrameOrigin(NSPoint::new(
                40.0,
                4.0 + (rows.len() - index - 1) as f64 * 44.0,
            ));
            row.remove.setTag(index as isize);
        }
        self.switch_card
            .setFrameOrigin(NSPoint::new(0.0, height - search_height - 120.0));
        self.app_shortcuts_card
            .setFrameOrigin(NSPoint::new(0.0, height - search_height - 200.0));
        self.command_shortcuts.view.setFrameOrigin(NSPoint::new(
            0.0,
            height - search_height - 216.0 - self.command_shortcuts.view.frame().size.height,
        ));
        self.add_search
            .setEnabled(rows.len() + 1 < winlane::core::config::MAX_SEARCH_SHORTCUTS);
    }

    pub fn add_search_shortcut(&self) {
        if self.search_rows.borrow().len() + 1 >= winlane::core::config::MAX_SEARCH_SHORTCUTS {
            return;
        }
        self.append_search_row(&Shortcut {
            control: false,
            option: false,
            shift: false,
            command: false,
            key: "Space".into(),
        });
        self.layout_search_shortcuts();
        if let Some(row) = self.search_rows.borrow().last() {
            row.view.scrollRectToVisible(row.view.bounds());
        }
        self.report(
            tr!(
                "请选择新增快捷键的修饰键和按键。",
                "Choose modifiers and a key for the new shortcut."
            ),
            false,
        );
    }

    pub fn remove_search_shortcut(&self, index: usize) {
        if index < self.search_rows.borrow().len() {
            self.search_rows
                .borrow_mut()
                .remove(index)
                .view
                .removeFromSuperview();
            self.layout_search_shortcuts();
        }
    }

    pub fn candidate(&self) -> Result<Config, String> {
        let config = Config {
            shortcut: self.search_shortcut.read()?,
            additional_search_shortcuts: self
                .search_rows
                .borrow()
                .iter()
                .map(|row| row.shortcut.read())
                .collect::<Result<_, _>>()?,
            switch_shortcut: self.switch_shortcut.read()?,
            app_shortcuts: self.app_shortcuts.borrow().clone(),
            command_shortcuts: self.command_shortcuts.read()?,
            alias_rules: self.alias_rules.borrow().clone(),
            snippets: self.snippets.borrow().clone(),
            quicklinks: self.quicklinks.borrow().clone(),
            clipboard: self.clipboard.read()?,
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
            display_density: match self.density.indexOfSelectedItem() {
                0 => DisplayDensity::Compact,
                _ => DisplayDensity::Normal,
            },
            background_opacity: self.read_opacity()?,
            show_usage_hints: self.usage_hints.state() == NSControlStateValueOn,
            switch_delay_ms: self
                .switch_delay
                .stringValue()
                .to_string()
                .trim()
                .parse()
                .map_err(|_| {
                    tr!(
                        "显示延迟应为 0–1000 的整数。",
                        "Display delay must be an integer from 0 to 1000."
                    )
                })?,
            input_method: match self.input_method.indexOfSelectedItem() {
                0 => InputMethod::Current,
                2 => InputMethod::Chinese,
                3 => InputMethod::LastUsed,
                _ => InputMethod::English,
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

    pub fn fill_clipboard(&self, settings: &winlane::features::clipboard::ClipboardSettings) {
        self.clipboard.fill(settings);
    }

    pub fn selected_tab(&self) -> isize {
        self.tabs
            .selectedTabViewItem()
            .map_or(0, |item| self.tabs.indexOfTabViewItem(&item))
    }

    fn read_opacity(&self) -> Result<u8, String> {
        self.opacity_input
            .stringValue()
            .to_string()
            .trim()
            .parse::<u8>()
            .ok()
            .filter(|value| *value <= 100)
            .ok_or_else(|| {
                tr!(
                    "请输入 0–100 之间的整数百分比。",
                    "Enter a whole-number percentage from 0 to 100."
                )
                .into()
            })
    }

    fn set_opacity(&self, value: u8) {
        self.opacity_slider.setDoubleValue(f64::from(value));
        self.opacity_input
            .setStringValue(&NSString::from_str(&value.to_string()));
        self.opacity_preview.set_opacity(f64::from(value) / 100.0);
    }

    pub fn opacity_slider_changed(&self) {
        self.set_opacity(self.opacity_slider.doubleValue().round() as u8);
    }

    pub fn opacity_input_changed(&self) -> Result<(), String> {
        self.set_opacity(self.read_opacity()?);
        Ok(())
    }

    pub fn select_tab(&self, index: isize) {
        if (0..self.tabs.numberOfTabViewItems()).contains(&index) {
            // End text editing before replacing its page so pending values still autosave.
            self.window.makeFirstResponder(None);
            self.tabs.selectTabViewItemAtIndex(index);
            self.layout_selected_tab();
        }
    }

    pub fn embed_quicklinks(&self, view: &NSView) {
        view.setFrame(self.quicklinks_tab.bounds());
        self.quicklinks_tab.addSubview(view);
    }
    pub fn set_quicklinks(&self, links: &[winlane::features::quicklinks::Quicklink]) {
        self.quicklinks.replace(links.to_vec());
    }

    pub fn embed_snippets(&self, view: &NSView) {
        view.setFrame(self.snippets_tab.bounds());
        self.snippets_tab.addSubview(view);
    }

    pub fn layout_selected_tab(&self) {
        let selected = self.selected_tab();
        for button in &self.navigation {
            button.setState(if button.tag() == selected {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
            NSView::setNeedsDisplay(button, true);
        }
        if let Some(item) = self.tabs.selectedTabViewItem() {
            self.page_title.setStringValue(&item.label());
        }
        self.page_description.setHidden(selected == 0);
        self.tabs
            .setFrameOrigin(NSPoint::new(252.0, if selected == 0 { 84.0 } else { 48.0 }));
        let description = match selected {
            0 => "",
            1 => tr!(
                "调整搜索与切换面板的外观、密度和透明度。",
                "Make search and switch panels feel right for you."
            ),
            2 => tr!(
                "统一设置 Winlane 所有输入框的默认输入法。",
                "Choose the default input source for all Winlane text fields."
            ),
            3 => tr!(
                "控制候选窗口、排列顺序和面板响应速度。",
                "Control which windows appear, their order, and when the switcher opens."
            ),
            4 => tr!(
                "管理 Winlane 的语言、启动行为和更新。",
                "Manage language, startup behavior, and updates."
            ),
            5 => tr!(
                "为常用应用和项目保留容易记住的 alias。",
                "Use memorable aliases for your apps and projects."
            ),
            6 => tr!(
                "创建可复用的文本，加入日期、剪贴板和自定义占位符。",
                "Reusable text with dates, clipboard content, and custom placeholders."
            ),
            7 => tr!(
                "管理复制的文本和图片，以及它们在本机的保留方式。",
                "Control how copied text and images are kept on this Mac."
            ),
            _ => tr!(
                "为网址、文件和常用搜索创建快捷入口。",
                "Shortcuts to websites, files, and your everyday searches."
            ),
        };
        self.page_description
            .setStringValue(&NSString::from_str(description));
    }

    pub fn set_snippets(&self, snippets: &[winlane::features::snippets::Snippet]) {
        self.snippets.replace(snippets.to_vec());
    }

    pub fn set_alias_rules(&self, rules: &[AliasRule]) {
        self.alias_rules.replace(rules.to_vec());
    }

    pub fn set_app_shortcuts(&self, shortcuts: &[AppShortcut]) {
        self.app_shortcuts.replace(shortcuts.to_vec());
    }

    pub fn set_command_shortcuts(&self, shortcuts: &[winlane::core::config::CommandShortcut]) {
        self.command_shortcuts.fill(shortcuts);
    }

    pub fn report(&self, message: &str, error: bool) {
        let color = if error {
            NSColor::systemRedColor()
        } else {
            NSColor::secondaryLabelColor()
        };
        self.message.setTextColor(Some(&color));
        let text = NSString::from_str(message);
        self.message.setStringValue(&text);
        self.message
            .setToolTip((!message.is_empty()).then_some(&text));
    }

    pub fn show(&self, config: &Config) {
        self.fill(config);
        self.report("", false);
        self.window.center();
        self.window.makeKeyAndOrderFront(None);
    }

    pub fn update_updater(&self, available: bool, automatic: bool, error: Option<&str>) {
        self.automatic_updates.setEnabled(available);
        self.automatic_updates.setState(if automatic {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        self.check_updates.setEnabled(available || error.is_some());
        let status = if error.is_some() {
            tr!(
                "更新组件未能启动。点击“检查更新”查看详情。",
                "The updater could not start. Choose Check for Updates for details."
            )
        } else if available {
            tr!(
                "每天检查一次；确认后才会下载、安装并重启。",
                "Checks once a day. Downloads, installation, and relaunch require your confirmation."
            )
        } else {
            tr!(
                "开发构建不检查更新。请使用发布版获取自动更新。",
                "Updates are disabled in development builds. Use a release build to receive updates."
            )
        };
        self.update_status
            .setStringValue(&NSString::from_str(status));
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

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../../tests/native/settings.rs"]
pub(crate) mod tests;
