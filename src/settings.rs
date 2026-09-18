use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::*;
use objc2_foundation::{
    MainThreadMarker, NSLocale, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
    NSUserDefaults, ns_string,
};
use objc2_service_management::{SMAppService, SMAppServiceStatus};
use std::cell::RefCell;
use winlane::aliases::Aliases;
use winlane::config::{
    AliasRule, AppShortcut, Appearance, Config, DisplayDensity, KEYS, Shortcut, SortOrder,
    key_label, parse_excluded,
};
use winlane::i18n::{self, Language};
use winlane::input_method::InputMethod;
use winlane::{tr, trf};

define_class!(
    // SAFETY: Settings windows and event dispatch stay on AppKit's main thread.
    #[unsafe(super = NSWindow)]
    #[thread_kind = MainThreadOnly]
    #[derive(Debug)]
    struct PreferencesWindow;
    unsafe impl NSObjectProtocol for PreferencesWindow {}
    impl PreferencesWindow {
        #[unsafe(method(sendEvent:))]
        fn send_event(&self, event: &NSEvent) {
            if event.r#type() == NSEventType::KeyDown
                && i64::from(event.keyCode()) == winlane::shortcuts::ESCAPE
                && !event.modifierFlags().intersects(
                    NSEventModifierFlags::Command | NSEventModifierFlags::Control
                        | NSEventModifierFlags::Option | NSEventModifierFlags::Shift,
                )
                && self.attachedSheet().is_none()
            {
                let composing = self.firstResponder()
                    .and_then(|responder| responder.downcast::<NSTextView>().ok())
                    .is_some_and(|editor| NSTextInputClient::hasMarkedText(&*editor));
                if !composing {
                    if !event.isARepeat() { self.performClose(None); }
                    return;
                }
            }
            // SAFETY: Other keys and input-method cancellation keep their native behavior.
            unsafe { let _: () = msg_send![super(self), sendEvent: event]; }
        }
    }
);

pub(crate) fn preferences_window(frame: NSRect, mtm: MainThreadMarker) -> Retained<NSWindow> {
    // SAFETY: The initialized main-thread window is retained by its settings owner across closes.
    unsafe {
        let window: Retained<PreferencesWindow> = msg_send![
            PreferencesWindow::alloc(mtm),
            initWithContentRect: frame,
            styleMask: NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ];
        window.setReleasedWhenClosed(false);
        window.into_super()
    }
}

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

pub fn save(config: &Config, defaults: &NSUserDefaults) -> Result<(), String> {
    let json = NSString::from_str(&config.to_json()?);
    // SAFETY: NSString is an accepted property-list value. One key stores the whole validated configuration.
    unsafe { defaults.setObject_forKey(Some(&json), storage_key()) };
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

    pub(crate) fn on_change(&self, target: &AnyObject, action: Sel) {
        for control in &self.modifiers {
            set_action(control, target, action);
        }
        set_action(&self.key, target, action);
    }

    pub(crate) fn notify_changed(&self) {
        // SAFETY: on_change installs a live delegate and its matching action.
        unsafe {
            self.key
                .sendAction_to(self.key.action(), self.key.target().as_deref())
        };
    }
}

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
    snippets_tab: Retained<NSView>,
    quicklinks_tab: Retained<NSView>,
    clipboard: crate::clipboard_settings::ClipboardControls,
    language: Retained<NSPopUpButton>,
    input_method: Retained<NSPopUpButton>,
    sort: Retained<NSPopUpButton>,
    appearance: Retained<NSPopUpButton>,
    density: Retained<NSPopUpButton>,
    opacity_slider: Retained<NSSlider>,
    opacity_input: Retained<NSTextField>,
    switch_delay: Retained<NSTextField>,
    opacity_preview: Retained<NSVisualEffectView>,
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
    snippets: RefCell<Vec<winlane::snippets::Snippet>>,
    quicklinks: RefCell<Vec<winlane::quicklinks::Quicklink>>,
}

impl SettingsWindow {
    pub fn new(target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let window = preferences_window(rect(0.0, 0.0, 800.0, 620.0), mtm);
        window.setTitle(&NSString::from_str(tr!("Winlane 设置", "Winlane Settings")));
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 800.0, 620.0));
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
        let tabs = NSTabView::initWithFrame(NSTabView::alloc(mtm), rect(20.0, 112.0, 760.0, 404.0));
        tabs.setTabViewType(NSTabViewType::TopTabsBezelBorder);
        tabs.setAutoresizingMask(NSAutoresizingMaskOptions::ViewHeightSizable);
        for child in view.subviews() {
            child.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        }
        view.addSubview(&tabs);
        let shortcuts_host = settings_tab(&tabs, tr!("快捷键", "Shortcuts"), mtm);
        let shortcuts_scroll =
            NSScrollView::initWithFrame(NSScrollView::alloc(mtm), shortcuts_host.bounds());
        shortcuts_scroll.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        shortcuts_scroll.setHasVerticalScroller(true);
        shortcuts_scroll.setAutohidesScrollers(true);
        shortcuts_scroll.setDrawsBackground(false);
        let shortcuts = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 660.0, 350.0));
        shortcuts_scroll.setDocumentView(Some(&shortcuts));
        shortcuts_host.addSubview(&shortcuts_scroll);
        let appearance_tab = settings_tab(&tabs, tr!("外观与语言", "Appearance"), mtm);
        let input_tab = settings_tab(&tabs, tr!("输入", "Input"), mtm);
        let windows = settings_tab(&tabs, tr!("窗口列表", "Windows"), mtm);
        let startup = settings_tab(&tabs, tr!("启动与更新", "Startup"), mtm);
        let aliases = settings_tab(&tabs, tr!("Alias 规则", "Aliases"), mtm);
        let snippets_tab = settings_tab(&tabs, tr!("文本片段", "Snippets"), mtm);
        let clipboard_tab = settings_tab(&tabs, tr!("剪贴板", "Clipboard"), mtm);
        let quicklinks_tab = settings_tab(&tabs, tr!("快捷链接", "Quicklinks"), mtm);
        let clipboard =
            crate::clipboard_settings::ClipboardControls::new(&clipboard_tab, target, mtm);
        aliases.addSubview(&label(
            tr!(
                "固定应用与项目的字母",
                "Keep familiar aliases for apps and projects"
            ),
            16.0,
            rect(30.0, 303.0, 600.0, 30.0),
            mtm,
        ));
        aliases.addSubview(&hint(tr!("例如 w → WeChat，ck → Code 的 ckb 项目。\n规则优先于自动分配，重启后保留；标题关键词不区分大小写。", "For example, w → WeChat, ck → the ckb project in Code.\nRules override automatic aliases and survive restarts. Title matching ignores case."), rect(30.0, 218.0, 600.0, 70.0), mtm));
        aliases.addSubview(&button(
            tr!("配置 Alias 规则…", "Alias Rules…"),
            target,
            sel!(showAliasRules:),
            rect(30.0, 166.0, 240.0, 32.0),
            mtm,
        ));

        let search_header =
            NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 244.0, 660.0, 106.0));
        shortcuts.addSubview(&search_header);
        let add_search = button(
            tr!("＋ 添加搜索快捷键", "＋ Add Search Shortcut"),
            target,
            sel!(addSearchShortcut:),
            rect(435.0, 70.0, 200.0, 28.0),
            mtm,
        );
        search_header.addSubview(&add_search);
        search_header.addSubview(&label(
            tr!("搜索模式", "Search mode"),
            14.0,
            rect(30.0, 71.0, 380.0, 24.0),
            mtm,
        ));
        let search_shortcut = ShortcutControls::at(&search_header, 39.0, false, mtm);
        search_header.addSubview(&hint(
            tr!(
                "可添加多个组合，均用于打开或关闭搜索；Enter 确认。",
                "Add shortcuts to open or close the same search panel; Enter selects."
            ),
            rect(30.0, 0.0, 600.0, 32.0),
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
            rect(30.0, 315.0, 215.0, 25.0),
            mtm,
        ));
        let language = popup(
            &[tr!("跟随系统", "System"), "中文", "English"],
            rect(260.0, 312.0, 370.0, 28.0),
            mtm,
        );
        appearance_tab.addSubview(&language);
        appearance_tab.addSubview(&hint(
            tr!(
                "跟随 macOS 的首选语言；更改立即生效。",
                "Use your preferred macOS language. Changes take effect immediately."
            ),
            rect(30.0, 274.0, 600.0, 32.0),
            mtm,
        ));
        appearance_tab.addSubview(&label(
            tr!("外观", "Appearance"),
            14.0,
            rect(30.0, 245.0, 215.0, 25.0),
            mtm,
        ));
        let appearance = popup(
            &[
                tr!("跟随系统", "System"),
                tr!("浅色", "Light"),
                tr!("深色", "Dark"),
            ],
            rect(260.0, 242.0, 370.0, 28.0),
            mtm,
        );
        appearance_tab.addSubview(&appearance);
        appearance_tab.addSubview(&label(
            tr!("显示密度", "Display density"),
            14.0,
            rect(30.0, 199.0, 215.0, 25.0),
            mtm,
        ));
        let density = popup(
            &[tr!("紧凑", "Compact"), tr!("标准", "Normal")],
            rect(260.0, 196.0, 370.0, 28.0),
            mtm,
        );
        density.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "显示密度",
            "Display density"
        ))));
        appearance_tab.addSubview(&density);
        appearance_tab.addSubview(&hint(
            tr!(
                "标准模式使用更大的文字、图标和行距，适用于搜索和切换面板。",
                "Normal uses larger text, icons, and rows in search and switch panels."
            ),
            rect(30.0, 158.0, 600.0, 30.0),
            mtm,
        ));

        appearance_tab.addSubview(&label(
            tr!("背景不透明度", "Background opacity"),
            14.0,
            rect(30.0, 119.0, 215.0, 25.0),
            mtm,
        ));
        let opacity_slider =
            NSSlider::initWithFrame(NSSlider::alloc(mtm), rect(260.0, 120.0, 275.0, 24.0));
        opacity_slider.setMinValue(0.0);
        opacity_slider.setMaxValue(100.0);
        opacity_slider.setContinuous(true);
        opacity_slider.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "背景不透明度",
            "Background opacity"
        ))));
        let opacity_input =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(550.0, 117.0, 58.0, 26.0));
        opacity_input.setAlignment(NSTextAlignment::Right);
        opacity_input.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "不透明度百分比",
            "Opacity percentage"
        ))));
        // SAFETY: The delegate outlives these controls and implements both actions.
        unsafe {
            opacity_slider.setTarget(Some(target));
            opacity_slider.setAction(Some(sel!(changeBackgroundOpacity:)));
            opacity_input.setTarget(Some(target));
            opacity_input.setAction(Some(sel!(commitBackgroundOpacity:)));
        }
        appearance_tab.addSubview(&opacity_slider);
        appearance_tab.addSubview(&opacity_input);
        appearance_tab.addSubview(&label("%", 13.0, rect(613.0, 119.0, 18.0, 24.0), mtm));
        let opacity_hint = NSTextField::wrappingLabelWithString(
            &NSString::from_str(tr!(
                "100% 保持当前效果；数值越低，背景越透明。文字和图标保持清晰，更改自动保存。",
                "100% keeps the current look. Lower values reveal more background. Text and icons stay clear. Changes save automatically."
            )),
            mtm,
        );
        opacity_hint.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        opacity_hint.setTextColor(Some(&NSColor::secondaryLabelColor()));
        opacity_hint.setFrame(rect(30.0, 74.0, 600.0, 36.0));
        opacity_hint.setMaximumNumberOfLines(2);
        appearance_tab.addSubview(&opacity_hint);
        let sample = NSView::initWithFrame(NSView::alloc(mtm), rect(30.0, 38.0, 600.0, 28.0));
        for (x, color) in [
            (0.0, NSColor::systemIndigoColor()),
            (300.0, NSColor::systemTealColor()),
        ] {
            let tile = NSBox::initWithFrame(NSBox::alloc(mtm), rect(x, 0.0, 300.0, 28.0));
            tile.setBoxType(NSBoxType::Custom);
            tile.setBorderWidth(0.0);
            tile.setFillColor(&color.colorWithAlphaComponent(0.35));
            sample.addSubview(&tile);
        }
        let opacity_preview = crate::app::panel_backdrop(sample.bounds(), mtm);
        opacity_preview.setBlendingMode(NSVisualEffectBlendingMode::WithinWindow);
        sample.addSubview(&opacity_preview);
        sample.addSubview(&label(
            tr!(
                "预览 · 搜索和切换面板",
                "Preview · Search and switch panels"
            ),
            14.0,
            rect(18.0, 3.0, 565.0, 23.0),
            mtm,
        ));
        appearance_tab.addSubview(&sample);
        let usage_hints = checkbox(
            tr!(
                "显示底部提示和设置按钮",
                "Show footer hints and Settings button"
            ),
            mtm,
        );
        usage_hints.setFrame(rect(30.0, 2.0, 600.0, 26.0));
        set_action(&usage_hints, target, sel!(settingsChanged:));
        appearance_tab.addSubview(&usage_hints);

        input_tab.addSubview(&label(
            tr!("搜索输入法", "Search input method"),
            14.0,
            rect(30.0, 315.0, 215.0, 25.0),
            mtm,
        ));
        let input_method = popup(
            &[
                tr!("跟随当前输入法", "Keep current input source"),
                tr!("始终英文", "Always English"),
                tr!("始终中文", "Always Chinese"),
                tr!("记住 Winlane 上次使用", "Last used in Winlane"),
            ],
            rect(260.0, 312.0, 370.0, 28.0),
            mtm,
        );
        input_method.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "搜索输入法",
            "Search input method"
        ))));
        input_tab.addSubview(&input_method);
        for (text, frame) in [
            (
                tr!(
                    "进入搜索框时应用，包括从切换模式按 Space 进入搜索。输入过程中仍可手动切换输入法。",
                    "Applied when you enter search, including Space from switch mode. You can still change input sources while typing."
                ),
                rect(30.0, 245.0, 600.0, 52.0),
            ),
            (
                tr!(
                    "英文 / 中文：使用 macOS 为该语言选择的已启用输入法，支持第三方输入法。",
                    "English / Chinese: use the enabled input source macOS chooses for that language, including third-party input methods."
                ),
                rect(30.0, 169.0, 600.0, 52.0),
            ),
            (
                tr!(
                    "记住上次：保存上次在 Winlane 搜索框中使用的输入法，重启后也会保留。",
                    "Last used: remember the input source used in Winlane search, even after restarting Winlane."
                ),
                rect(30.0, 101.0, 600.0, 52.0),
            ),
            (
                tr!(
                    "所需输入法未启用或已移除时，保留当前输入法。切换模式的 alias 不受影响。",
                    "If the requested input source is unavailable, keep the current one. Switch-mode aliases are unaffected."
                ),
                rect(30.0, 33.0, 600.0, 52.0),
            ),
        ] {
            let text = NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm);
            text.setFont(Some(&NSFont::systemFontOfSize(12.0)));
            text.setTextColor(Some(&NSColor::secondaryLabelColor()));
            text.setFrame(frame);
            input_tab.addSubview(&text);
        }

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
        set_action(&excluded, target, sel!(settingsChanged:));
        excluded.cell().unwrap().setSendsActionOnEndEditing(true);
        opacity_input
            .cell()
            .unwrap()
            .setSendsActionOnEndEditing(true);
        search_shortcut.on_change(target, sel!(settingsChanged:));
        switch_shortcut.on_change(target, sel!(settingsChanged:));
        for control in [&*language, &*appearance, &*density, &*sort, &*input_method] {
            set_action(control, target, sel!(settingsChanged:));
        }
        set_action(&minimized, target, sel!(settingsChanged:));
        windows.addSubview(&hint(
            tr!(
                "填写列表里显示的完整应用名，用逗号分隔。",
                "Enter app names exactly as shown in the list, separated by commas."
            ),
            rect(30.0, 85.0, 600.0, 38.0),
            mtm,
        ));

        windows.addSubview(&label(
            tr!("切换面板显示延迟（毫秒）", "Switcher display delay (ms)"),
            13.0,
            rect(30.0, 45.0, 360.0, 24.0),
            mtm,
        ));
        let switch_delay =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(480.0, 45.0, 150.0, 26.0));
        switch_delay
            .cell()
            .unwrap()
            .setSendsActionOnEndEditing(true);
        set_action(&switch_delay, target, sel!(settingsChanged:));
        windows.addSubview(&switch_delay);
        windows.addSubview(&hint(
            tr!(
                "快速松键直接切换；0 表示立即显示。",
                "Quick releases switch directly; 0 shows the panel immediately."
            ),
            rect(30.0, 8.0, 600.0, 30.0),
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
        let automatic_updates =
            checkbox(tr!("自动检查更新", "Automatically check for updates"), mtm);
        automatic_updates.setFrame(rect(30.0, 123.0, 375.0, 27.0));
        set_action(&automatic_updates, target, sel!(toggleAutomaticUpdates:));
        automatic_updates.setEnabled(false);
        startup.addSubview(&automatic_updates);
        let check_updates = button(
            tr!("检查更新…", "Check for Updates…"),
            target,
            sel!(checkForUpdates:),
            rect(430.0, 122.0, 200.0, 28.0),
            mtm,
        );
        check_updates.setEnabled(false);
        startup.addSubview(&check_updates);
        let update_status = hint("", rect(30.0, 49.0, 600.0, 62.0), mtm);
        update_status.setMaximumNumberOfLines(3);
        startup.addSubview(&update_status);

        let message = NSTextField::wrappingLabelWithString(ns_string!(""), mtm);
        message.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        message.setFrame(rect(28.0, 62.0, 664.0, 38.0));
        message.setMaximumNumberOfLines(2);
        view.addSubview(&message);
        view.addSubview(&button(
            tr!("恢复默认设置", "Restore Defaults"),
            target,
            sel!(resetSettings:),
            rect(28.0, 20.0, 190.0, 30.0),
            mtm,
        ));
        // SAFETY: The application delegate outlives the settings window and handles tab changes.
        unsafe {
            let _: () = msg_send![&tabs, setDelegate: target];
        }
        Self {
            window,
            tabs,
            snippets_tab,
            quicklinks_tab,
            clipboard,
            language,
            input_method,
            search_shortcut,
            search_rows: RefCell::default(),
            shortcuts_document: shortcuts,
            shortcuts_scroll,
            search_header,
            add_search,
            switch_shortcut,
            sort,
            appearance,
            density,
            opacity_slider,
            opacity_input,
            switch_delay,
            opacity_preview,
            usage_hints,
            minimized,
            excluded,
            login,
            login_status,
            automatic_updates,
            check_updates,
            update_status,
            message,
            app_shortcuts: RefCell::default(),
            alias_rules: RefCell::default(),
            snippets: RefCell::default(),
            quicklinks: RefCell::default(),
        }
    }

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
        self.shortcuts_document.addSubview(&view);
        self.search_rows.borrow_mut().push(SearchShortcutRow {
            view,
            shortcut,
            remove,
        });
    }

    fn layout_search_shortcuts(&self) {
        let rows = self.search_rows.borrow();
        let height =
            self.shortcuts_scroll.contentSize().height.max(350.0) + rows.len() as f64 * 40.0;
        self.shortcuts_document
            .setFrameSize(NSSize::new(660.0, height));
        self.search_header
            .setFrameOrigin(NSPoint::new(0.0, height - 106.0));
        for (index, row) in rows.iter().enumerate() {
            row.view.setFrameOrigin(NSPoint::new(
                0.0,
                height - 106.0 - (index + 1) as f64 * 40.0,
            ));
            row.remove.setTag(index as isize);
        }
        self.add_search
            .setEnabled(rows.len() + 1 < winlane::config::MAX_SEARCH_SHORTCUTS);
    }

    pub fn add_search_shortcut(&self) {
        if self.search_rows.borrow().len() + 1 >= winlane::config::MAX_SEARCH_SHORTCUTS {
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

    pub fn fill_clipboard(&self, settings: &winlane::clipboard::ClipboardSettings) {
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
        self.opacity_preview.setAlphaValue(f64::from(value) / 100.0);
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
            self.tabs.selectTabViewItemAtIndex(index);
            self.layout_selected_tab();
        }
    }

    pub fn embed_quicklinks(&self, view: &NSView) {
        view.setFrame(self.quicklinks_tab.bounds());
        self.quicklinks_tab.addSubview(view);
    }
    pub fn set_quicklinks(&self, links: &[winlane::quicklinks::Quicklink]) {
        self.quicklinks.replace(links.to_vec());
    }

    pub fn embed_snippets(&self, view: &NSView) {
        view.setFrame(self.snippets_tab.bounds());
        self.snippets_tab.addSubview(view);
    }

    pub fn layout_selected_tab(&self) {
        let height = if self.selected_tab() == 6 {
            800.0
        } else {
            620.0
        };
        let Some(view) = self.window.contentView() else {
            return;
        };
        if (view.frame().size.height - height).abs() < 0.5 {
            return;
        }
        self.window.makeFirstResponder(None);
        let old_frame = self.window.frame();
        self.window.setContentSize(NSSize::new(800.0, height));
        let frame = self.window.frame();
        let mut origin = NSPoint::new(
            old_frame.origin.x,
            old_frame.origin.y + old_frame.size.height - frame.size.height,
        );
        if let Some(screen) = self.window.screen() {
            origin.y = origin.y.max(screen.visibleFrame().origin.y);
        }
        self.window.setFrameOrigin(origin);
    }

    pub fn set_snippets(&self, snippets: &[winlane::snippets::Snippet]) {
        self.snippets.replace(snippets.to_vec());
    }

    pub fn set_alias_rules(&self, rules: &[AliasRule]) {
        self.alias_rules.replace(rules.to_vec());
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
                "更改自动保存；文字输入在按 Return 或结束编辑时保存。",
                "Changes save automatically. Text fields save on Return or when editing ends."
            ),
            false,
        );
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
pub(crate) fn checkbox(title: &str, mtm: MainThreadMarker) -> Retained<NSButton> {
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
pub(crate) fn set_action(button: &NSControl, target: &AnyObject, action: Sel) {
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
