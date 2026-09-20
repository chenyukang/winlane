mod appearance;
mod command_shortcuts;
mod input;
mod input_rules;
mod layout;
mod navigation;
mod pages;
mod shortcuts;

use super::controls::*;
use super::shortcut::ShortcutControls;
use appearance::AppearancePage;
use input::InputPage;
use input_rules::InputRulesPage;
use navigation::SettingsNavigationButton;
use objc2::rc::{Retained, Weak};
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};
use objc2_service_management::{SMAppService, SMAppServiceStatus};
use pages::{GeneralPage, WindowsPage};
use shortcuts::ShortcutsPage;
use std::cell::{OnceCell, RefCell};
use winlane::core::config::{
    Appearance, Config, DisplayDensity, Shortcut, SortOrder, parse_excluded,
};
use winlane::core::i18n::Language;
#[cfg(test)]
use winlane::core::input_method::InputMethod;
use winlane::{tr, trf};

struct SearchShortcutRow {
    view: Retained<NSView>,
    shortcut: ShortcutControls,
    remove: Retained<NSButton>,
}

pub struct SettingsWindow {
    pub window: Retained<NSWindow>,
    target: Weak<AnyObject>,
    config: RefCell<Config>,
    shortcuts: OnceCell<ShortcutsPage>,
    appearance: OnceCell<AppearancePage>,
    general: OnceCell<GeneralPage>,
    input: OnceCell<InputPage>,
    input_rules: OnceCell<InputRulesPage>,
    windows: OnceCell<WindowsPage>,
    clipboard: OnceCell<crate::macos::ui::clipboard_settings::ClipboardControls>,
    updater: RefCell<(bool, bool, Option<String>)>,
    tabs: Retained<NSTabView>,
    navigation: Vec<Retained<SettingsNavigationButton>>,
    page_title: Retained<NSTextField>,
    page_description: Retained<NSTextField>,
    message: Retained<NSTextField>,
}

impl SettingsWindow {
    pub fn fill(&self, config: &Config) {
        self.config.replace(config.clone());
        if let Some(page) = self.shortcuts.get() {
            page.fill(config);
        }
        if let Some(page) = self.appearance.get() {
            page.fill(config);
        }
        if let Some(page) = self.general.get() {
            page.fill(config);
        }
        if let Some(page) = self.input.get() {
            page.fill(config);
        }
        if let Some(page) = self.input_rules.get() {
            page.fill(config);
        }
        if let Some(page) = self.windows.get() {
            page.fill(config);
        }
        if let Some(page) = self.clipboard.get() {
            page.fill(&config.clipboard);
        }
    }

    pub fn sync_saved_config(&self, config: &Config) {
        self.config.replace(config.clone());
        if let Some(page) = self.input.get() {
            page.sync_controls();
        }
        if let Some(page) = self.shortcuts.get() {
            page.command_shortcuts.fill(&config.command_shortcuts);
        }
        if let Some(page) = self.clipboard.get() {
            page.fill(&config.clipboard);
        }
    }

    pub fn refresh_input_sources(&self) {
        if let Some(page) = self.input_rules.get() {
            page.refresh_sources();
        }
        if let Some(page) = self.input.get() {
            page.refresh_sources(&self.config.borrow());
        }
    }
    pub fn indicator_source_selected(&self) {
        if let Some(page) = self.input.get() {
            page.select_source(&self.config.borrow());
        }
    }
    pub fn reset_indicator_color(&self) {
        if let Some(page) = self.input.get() {
            page.reset_color();
        }
    }

    fn host(&self, index: isize) -> Retained<NSView> {
        self.tabs
            .tabViewItemAtIndex(index)
            .view(self.window.mtm())
            .unwrap()
    }
    fn shortcuts(&self) -> &ShortcutsPage {
        self.shortcuts.get_or_init(|| {
            let target = self.target.load().expect("settings target is alive");
            let page = ShortcutsPage::new(&self.host(0), &target, self.window.mtm());
            page.fill(&self.config.borrow());
            page
        })
    }
    fn appearance(&self) -> &AppearancePage {
        self.appearance.get_or_init(|| {
            let target = self.target.load().expect("settings target is alive");
            let page = AppearancePage::new(&self.host(1), &target, self.window.mtm());
            page.fill(&self.config.borrow());
            page
        })
    }
    fn input(&self) -> &InputPage {
        self.input.get_or_init(|| {
            let target = self.target.load().expect("settings target is alive");
            let page = InputPage::new(&self.host(2), &target, self.window.mtm());
            page.fill(&self.config.borrow());
            page
        })
    }
    fn input_rules(&self) -> &InputRulesPage {
        self.input_rules.get_or_init(|| {
            let target = self.target.load().expect("settings target is alive");
            let page = InputRulesPage::new(&self.host(9), &target, self.window.mtm());
            page.fill(&self.config.borrow());
            page
        })
    }
    pub fn select_input_rule(&self, index: usize) {
        self.input_rules().select(index);
    }
    pub fn add_input_rule(&self) {
        self.input_rules().add(&self.window, &self.message);
    }
    pub fn remove_input_rule(&self, index: usize) {
        self.input_rules().remove(index);
    }
    pub fn choose_input_rule_app(&self, index: usize) {
        self.input_rules()
            .choose(index, &self.window, &self.message);
    }
    fn windows(&self) -> &WindowsPage {
        self.windows.get_or_init(|| {
            let target = self.target.load().expect("settings target is alive");
            let page = WindowsPage::new(&self.host(3), &target, self.window.mtm());
            page.fill(&self.config.borrow());
            page
        })
    }
    fn general(&self) -> &GeneralPage {
        self.general.get_or_init(|| {
            let target = self.target.load().expect("settings target is alive");
            let page = GeneralPage::new(&self.host(4), &target, self.window.mtm());
            page.fill(&self.config.borrow());
            let updater = self.updater.borrow();
            page.update_updater(updater.0, updater.1, updater.2.as_deref());
            page
        })
    }
    fn ensure_page(&self, index: isize) {
        match index {
            0 => {
                self.shortcuts();
            }
            1 => {
                self.appearance();
            }
            2 => {
                self.input();
            }
            3 => {
                self.windows();
            }
            4 => {
                self.general();
            }
            7 => {
                self.clipboard.get_or_init(|| {
                    let target = self.target.load().expect("settings target is alive");
                    let page = crate::macos::ui::clipboard_settings::ClipboardControls::new(
                        &self.host(7),
                        &target,
                        self.window.mtm(),
                    );
                    page.fill(&self.config.borrow().clipboard);
                    page
                });
            }
            9 => {
                self.input_rules();
            }
            _ => {}
        }
    }

    pub fn candidate(&self) -> Result<Config, String> {
        // Unvisited pages contribute their saved values, never control defaults.
        let mut config = self.config.borrow().clone();
        if let Some(page) = self.shortcuts.get() {
            page.read(&mut config)?;
        }
        if let Some(page) = self.appearance.get() {
            page.read(&mut config)?;
        }
        if let Some(page) = self.windows.get() {
            page.read(&mut config)?;
        }
        if let Some(page) = self.general.get() {
            config.language = match page.language.indexOfSelectedItem() {
                1 => Language::Chinese,
                2 => Language::English,
                _ => Language::System,
            };
        }
        if let Some(page) = self.input.get() {
            page.read(&mut config)?;
        }
        if let Some(page) = self.clipboard.get() {
            config.clipboard = page.read()?;
        }
        if let Some(page) = self.input_rules.get() {
            page.read(&mut config);
        }
        config.validate()?;
        Ok(config)
    }
    pub fn add_search_shortcut(&self) {
        let page = self.shortcuts();
        if page.search_rows.borrow().len() + 1 >= winlane::core::config::MAX_SEARCH_SHORTCUTS {
            return;
        }
        page.append_search_row(&Shortcut {
            control: false,
            option: false,
            shift: false,
            command: false,
            key: "Space".into(),
        });
        page.layout_search_shortcuts();
        if let Some(row) = page.search_rows.borrow().last() {
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
        let page = self.shortcuts();
        if index < page.search_rows.borrow().len() {
            page.search_rows
                .borrow_mut()
                .remove(index)
                .view
                .removeFromSuperview();
            page.layout_search_shortcuts();
        }
    }
    pub fn opacity_slider_changed(&self) {
        let page = self.appearance();
        page.set_opacity(page.opacity_slider.doubleValue().round() as u8);
    }
    pub fn opacity_input_changed(&self) -> Result<(), String> {
        let page = self.appearance();
        page.set_opacity(page.read_opacity()?);
        Ok(())
    }
    pub fn selected_tab(&self) -> isize {
        self.tabs
            .selectedTabViewItem()
            .map_or(0, |item| self.tabs.indexOfTabViewItem(&item))
    }
    pub fn select_tab(&self, index: isize) {
        if (0..self.tabs.numberOfTabViewItems()).contains(&index) {
            // End text editing before replacing its page so pending values still autosave.
            self.window.makeFirstResponder(None);
            self.ensure_page(index);
            self.tabs.selectTabViewItemAtIndex(index);
            self.layout_selected_tab();
        }
    }
    pub fn layout_selected_tab(&self) {
        let selected = self.selected_tab();
        self.ensure_page(selected);
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
                "配置屏幕常驻输入法指示器的外观和位置。",
                "Choose the appearance and position of the input source indicator."
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
            9 => tr!(
                "为 Winlane 和其他应用统一管理输入法规则。",
                "Manage input source rules for Winlane and other apps."
            ),
            _ => tr!(
                "为网址、文件和常用搜索创建快捷入口。",
                "Shortcuts to websites, files, and your everyday searches."
            ),
        };
        self.page_description
            .setStringValue(&NSString::from_str(description));
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
    pub fn embed_alias_rules(&self, view: &NSView) {
        let host = self.host(5);
        view.setFrame(host.bounds());
        host.addSubview(view);
    }
    pub fn embed_snippets(&self, view: &NSView) {
        let host = self.host(6);
        view.setFrame(host.bounds());
        host.addSubview(view);
    }
    pub fn embed_quicklinks(&self, view: &NSView) {
        let host = self.host(8);
        view.setFrame(host.bounds());
        host.addSubview(view);
    }
    pub fn show(&self, config: &Config) {
        self.fill(config);
        self.select_tab(self.selected_tab());
        self.report("", false);
        self.bring_to_front();
    }
    pub fn bring_to_front(&self) {
        if self.window.isMiniaturized() {
            self.window.deminiaturize(None);
        }
        self.window.orderFrontRegardless();
        self.window.makeKeyAndOrderFront(None);
    }
    pub fn update_updater(&self, available: bool, automatic: bool, error: Option<&str>) {
        self.updater
            .replace((available, automatic, error.map(str::to_owned)));
        if let Some(page) = self.general.get() {
            page.update_updater(available, automatic, error);
        }
    }
    pub fn update_login_status(&self) {
        if let Some(page) = self.general.get() {
            page.update_login_status();
        }
    }
    pub fn toggle_login(&self) {
        // SAFETY: Invoked only by the user's explicit login-item switch action, on the main thread.
        let result = unsafe {
            let service = SMAppService::mainAppService();
            if self.general().login.state() == NSControlStateValueOn {
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
