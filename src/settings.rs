use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
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

pub(crate) const RECENT_WINDOW_LIMIT: usize = 128;

pub(crate) fn load_recency(defaults: &NSUserDefaults) -> Vec<u64> {
    let mut recent = defaults
        .stringForKey(ns_string!("WinlaneRecentWindowsV1"))
        .and_then(|value| serde_json::from_str::<Vec<u64>>(&value.to_string()).ok())
        .unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    recent.retain(|id| seen.insert(*id));
    recent.truncate(RECENT_WINDOW_LIMIT);
    recent
}

pub(crate) fn save_recency(recent: &[u64], defaults: &NSUserDefaults) {
    let recent = &recent[..recent.len().min(RECENT_WINDOW_LIMIT)];
    let json = NSString::from_str(&serde_json::to_string(recent).unwrap());
    // SAFETY: Only window IDs are stored, separately from settings and aliases.
    unsafe {
        defaults.setObject_forKey(Some(&json), ns_string!("WinlaneRecentWindowsV1"));
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

#[derive(Debug)]
struct NavigationStyle {
    color: Retained<NSColor>,
    symbol: Option<Retained<NSImage>>,
}

define_class!(
    // SAFETY: Settings navigation and drawing stay on AppKit's main thread.
    #[unsafe(super = NSButton)]
    #[thread_kind = MainThreadOnly]
    #[ivars = NavigationStyle]
    #[derive(Debug)]
    struct SettingsNavigationButton;
    unsafe impl NSObjectProtocol for SettingsNavigationButton {}
    impl SettingsNavigationButton {
        #[unsafe(method(drawRect:))]
        fn draw(&self, dirty: NSRect) {
            if self.state() == NSControlStateValueOn || self.isHighlighted() {
                NSColor::labelColor().colorWithAlphaComponent(0.09).setFill();
                NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(self.bounds(), 9.0, 9.0).fill();
            }
            // SAFETY: The native button draws its title and keyboard focus indication.
            unsafe { let _: () = msg_send![super(self), drawRect: dirty]; }
            self.ivars().color.setFill();
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect(10.0, 7.0, 26.0, 26.0), 6.0, 6.0).fill();
            if let Some(symbol) = &self.ivars().symbol {
                // SAFETY: No drawing hints are provided; respect the native button's flipped coordinates.
                unsafe { symbol.drawInRect_fromRect_operation_fraction_respectFlipped_hints(rect(14.0, 11.0, 18.0, 18.0), NSRect::ZERO, NSCompositingOperation::SourceOver, 1.0, true, None); }
            }
        }
    }
);

impl SettingsNavigationButton {
    fn new(
        title: &str,
        symbol: &str,
        color: Retained<NSColor>,
        frame: NSRect,
        target: &AnyObject,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let symbol = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(symbol),
            None,
        )
        .and_then(|image| {
            image.imageWithSymbolConfiguration(
                &NSImageSymbolConfiguration::configurationWithHierarchicalColor(
                    &NSColor::whiteColor(),
                ),
            )
        });
        let this = Self::alloc(mtm).set_ivars(NavigationStyle { color, symbol });
        // SAFETY: The button is initialized once with its main-thread-owned drawing state.
        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        this.setTitle(&NSString::from_str(&format!("           {title}")));
        this.setFont(Some(&NSFont::systemFontOfSize(14.0)));
        this.setAlignment(NSTextAlignment::Left);
        this.setBordered(false);
        this.setButtonType(NSButtonType::MomentaryChange);
        this.setAccessibilityLabel(Some(&NSString::from_str(title)));
        set_action(&this, target, sel!(selectSettingsSection:));
        this
    }
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
    opacity_preview: crate::app::PanelBackdrop,
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
        let window = preferences_window(rect(0.0, 0.0, 1020.0, 740.0), mtm);
        window.setTitle(&NSString::from_str(tr!("Winlane 设置", "Winlane Settings")));
        window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
        window.setTitlebarAppearsTransparent(true);
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 1020.0, 740.0));
        window.setContentView(Some(&view));
        let sidebar = NSVisualEffectView::initWithFrame(
            NSVisualEffectView::alloc(mtm),
            rect(0.0, 0.0, 220.0, 740.0),
        );
        sidebar.setMaterial(NSVisualEffectMaterial::Sidebar);
        sidebar.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        view.addSubview(&sidebar);
        let brand = label("Winlane", 21.0, rect(24.0, 677.0, 178.0, 32.0), mtm);
        brand.setFont(Some(&NSFont::boldSystemFontOfSize(21.0)));
        sidebar.addSubview(&brand);
        sidebar.addSubview(&hint(
            env!("CARGO_PKG_VERSION"),
            rect(25.0, 654.0, 175.0, 22.0),
            mtm,
        ));
        sidebar.addSubview(&hint(
            tr!("偏好设置", "PREFERENCES"),
            rect(24.0, 618.0, 178.0, 20.0),
            mtm,
        ));
        sidebar.addSubview(&hint(
            tr!("工具", "TOOLS"),
            rect(24.0, 314.0, 178.0, 20.0),
            mtm,
        ));
        let divider = NSBox::initWithFrame(NSBox::alloc(mtm), rect(219.0, 0.0, 1.0, 740.0));
        divider.setBoxType(NSBoxType::Separator);
        view.addSubview(&divider);
        let page_title = label("", 25.0, rect(252.0, 677.0, 740.0, 36.0), mtm);
        page_title.setFont(Some(&NSFont::boldSystemFontOfSize(25.0)));
        view.addSubview(&page_title);
        let page_description = hint("", rect(252.0, 632.0, 740.0, 38.0), mtm);
        page_description.setFont(Some(&NSFont::systemFontOfSize(13.0)));
        view.addSubview(&page_description);
        let tabs = NSTabView::initWithFrame(NSTabView::alloc(mtm), rect(252.0, 48.0, 740.0, 574.0));
        tabs.setTabViewType(NSTabViewType::NoTabsNoBorder);
        tabs.setDrawsBackground(false);
        view.addSubview(&tabs);
        let shortcuts_host = settings_tab(&tabs, tr!("快捷键", "Shortcuts"), mtm);
        let appearance_tab = settings_tab(&tabs, tr!("外观", "Appearance"), mtm);
        let input_tab = settings_tab(&tabs, tr!("输入", "Input"), mtm);
        let windows = settings_tab(&tabs, tr!("窗口列表", "Windows"), mtm);
        let general = settings_tab(&tabs, tr!("常规", "General"), mtm);
        let aliases = settings_tab(&tabs, tr!("Alias 规则", "Aliases"), mtm);
        let snippets_tab = settings_tab(&tabs, tr!("文本片段", "Snippets"), mtm);
        let clipboard_tab = settings_tab(&tabs, tr!("剪贴板", "Clipboard"), mtm);
        let quicklinks_tab = settings_tab(&tabs, tr!("快捷链接", "Quicklinks"), mtm);
        let mut navigation = Vec::new();
        for (position, (index, symbol, color)) in [
            (4, "gearshape.fill", NSColor::systemGrayColor()),
            (1, "paintpalette.fill", NSColor::systemPinkColor()),
            (0, "keyboard", NSColor::systemPurpleColor()),
            (2, "character.cursor.ibeam", NSColor::systemBlueColor()),
            (3, "macwindow.on.rectangle", NSColor::systemIndigoColor()),
            (5, "textformat.abc", NSColor::systemTealColor()),
            (6, "text.quote", NSColor::systemGreenColor()),
            (7, "doc.on.clipboard", NSColor::systemOrangeColor()),
            (8, "link", NSColor::systemCyanColor()),
        ]
        .into_iter()
        .enumerate()
        {
            let y = if position < 6 {
                568.0 - position as f64 * 44.0
            } else {
                264.0 - (position - 6) as f64 * 44.0
            };
            let item = tabs.tabViewItemAtIndex(index);
            let control = SettingsNavigationButton::new(
                &item.label().to_string(),
                symbol,
                color,
                rect(12.0, y, 196.0, 40.0),
                target,
                mtm,
            );
            control.setTag(index);
            sidebar.addSubview(&control);
            navigation.push(control);
        }
        sidebar.addSubview(&button(
            tr!("恢复默认设置", "Restore Defaults"),
            target,
            sel!(resetSettings:),
            rect(16.0, 20.0, 188.0, 30.0),
            mtm,
        ));

        let shortcuts_scroll =
            NSScrollView::initWithFrame(NSScrollView::alloc(mtm), shortcuts_host.bounds());
        shortcuts_scroll.setHasVerticalScroller(true);
        shortcuts_scroll.setAutohidesScrollers(true);
        shortcuts_scroll.setScrollerStyle(NSScrollerStyle::Overlay);
        shortcuts_scroll.setDrawsBackground(false);
        let shortcuts = NSView::initWithFrame(NSView::alloc(mtm), shortcuts_host.bounds());
        shortcuts_scroll.setDocumentView(Some(&shortcuts));
        shortcuts_host.addSubview(&shortcuts_scroll);
        let search_card = card(&shortcuts, rect(0.0, 470.0, 740.0, 104.0), mtm);
        let search_header =
            NSView::initWithFrame(NSView::alloc(mtm), rect(40.0, 0.0, 660.0, 104.0));
        search_card.addSubview(&search_header);
        let add_search = button(
            tr!("＋ 添加搜索快捷键", "＋ Add Search Shortcut"),
            target,
            sel!(addSearchShortcut:),
            rect(435.0, 60.0, 200.0, 28.0),
            mtm,
        );
        search_header.addSubview(&add_search);
        search_header.addSubview(&label(
            tr!("搜索模式", "Search mode"),
            15.0,
            rect(30.0, 60.0, 380.0, 24.0),
            mtm,
        ));
        let search_shortcut = ShortcutControls::at(&search_header, 19.0, false, mtm);
        let switch_card = card(&shortcuts, rect(0.0, 350.0, 740.0, 104.0), mtm);
        let switch_content =
            NSView::initWithFrame(NSView::alloc(mtm), rect(40.0, 0.0, 660.0, 104.0));
        switch_card.addSubview(&switch_content);
        switch_content.addSubview(&label(
            tr!("切换模式", "Switch mode"),
            15.0,
            rect(30.0, 60.0, 600.0, 24.0),
            mtm,
        ));
        let switch_shortcut = ShortcutControls::at(&switch_content, 19.0, true, mtm);
        let app_shortcuts_card = card(&shortcuts, rect(0.0, 270.0, 740.0, 64.0), mtm);
        app_shortcuts_card.addSubview(&label(
            tr!("应用快捷键", "App shortcuts"),
            15.0,
            rect(24.0, 20.0, 470.0, 25.0),
            mtm,
        ));
        app_shortcuts_card.addSubview(&button(
            tr!("配置应用快捷键…", "App Shortcuts…"),
            target,
            sel!(showAppShortcuts:),
            rect(510.0, 17.0, 208.0, 30.0),
            mtm,
        ));

        let localization = settings_group(&general, tr!("语言", "Language"), 570.0, 80.0, mtm);
        row_text(
            &localization,
            tr!("界面语言", "Interface language"),
            tr!("更改立即生效。", "Changes take effect immediately."),
            80.0,
            390.0,
            mtm,
        );
        let language = popup(
            &[tr!("跟随系统", "System"), "中文", "English"],
            rect(420.0, 25.0, 300.0, 28.0),
            mtm,
        );
        localization.addSubview(&language);
        let startup = settings_group(&general, tr!("启动", "Startup"), 436.0, 114.0, mtm);
        let login = checkbox(tr!("登录时自动启动", "Launch at login"), mtm);
        login.setFrame(rect(20.0, 72.0, 360.0, 26.0));
        set_action(&login, target, sel!(toggleLogin:));
        startup.addSubview(&login);
        startup.addSubview(&button(
            tr!("管理登录项…", "Manage Login Items…"),
            target,
            sel!(manageLogin:),
            rect(500.0, 69.0, 220.0, 30.0),
            mtm,
        ));
        let login_status = hint("", rect(22.0, 18.0, 696.0, 44.0), mtm);
        startup.addSubview(&login_status);
        let updates = settings_group(&general, tr!("更新", "Updates"), 268.0, 116.0, mtm);
        let automatic_updates =
            checkbox(tr!("自动检查更新", "Automatically check for updates"), mtm);
        automatic_updates.setFrame(rect(20.0, 73.0, 448.0, 27.0));
        set_action(&automatic_updates, target, sel!(toggleAutomaticUpdates:));
        automatic_updates.setEnabled(false);
        updates.addSubview(&automatic_updates);
        let check_updates = button(
            tr!("检查更新…", "Check for Updates…"),
            target,
            sel!(checkForUpdates:),
            rect(500.0, 71.0, 220.0, 28.0),
            mtm,
        );
        check_updates.setEnabled(false);
        updates.addSubview(&check_updates);
        let update_status = hint("", rect(22.0, 14.0, 696.0, 48.0), mtm);
        update_status.setMaximumNumberOfLines(3);
        updates.addSubview(&update_status);
        general.addSubview(&hint(
            tr!(
                "设置保存在本机；窗口标题与搜索历史不会保存。",
                "Settings stay on this Mac. Window titles and search history are not saved."
            ),
            rect(16.0, 43.0, 708.0, 42.0),
            mtm,
        ));

        let display = settings_group(&appearance_tab, tr!("显示", "Display"), 570.0, 192.0, mtm);
        display.addSubview(&label(
            tr!("外观", "Appearance"),
            14.0,
            rect(20.0, 150.0, 300.0, 24.0),
            mtm,
        ));
        let appearance = popup(
            &[
                tr!("跟随系统", "System"),
                tr!("浅色", "Light"),
                tr!("深色", "Dark"),
            ],
            rect(420.0, 148.0, 300.0, 28.0),
            mtm,
        );
        display.addSubview(&appearance);
        row_divider(&display, 128.0, mtm);
        row_text(
            &display,
            tr!("显示密度", "Display density"),
            tr!(
                "标准模式使用更大的文字和图标。",
                "Normal uses larger text and icons."
            ),
            124.0,
            380.0,
            mtm,
        );
        let density = popup(
            &[tr!("紧凑", "Compact"), tr!("标准", "Normal")],
            rect(420.0, 73.0, 300.0, 28.0),
            mtm,
        );
        density.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "显示密度",
            "Display density"
        ))));
        display.addSubview(&density);
        row_divider(&display, 49.0, mtm);
        let usage_hints = checkbox(
            tr!(
                "显示底部提示和设置按钮",
                "Show footer hints and Settings button"
            ),
            mtm,
        );
        usage_hints.setFrame(rect(20.0, 13.0, 690.0, 26.0));
        set_action(&usage_hints, target, sel!(settingsChanged:));
        display.addSubview(&usage_hints);
        let opacity = settings_group(
            &appearance_tab,
            tr!("背景不透明度", "Background opacity"),
            324.0,
            196.0,
            mtm,
        );
        let glass = crate::app::glass_available();
        opacity.addSubview(&hint(
            if glass {
                tr!(
                    "正在使用 Liquid Glass，透明度由 macOS 自动调整。",
                    "Liquid Glass is active. macOS adjusts its transparency automatically."
                )
            } else {
                tr!(
                    "降低百分比可透出更多背景；文字和图标保持清晰。",
                    "Lower values reveal more background. Text and icons stay clear."
                )
            },
            rect(20.0, 148.0, 700.0, 30.0),
            mtm,
        ));
        let opacity_slider =
            NSSlider::initWithFrame(NSSlider::alloc(mtm), rect(20.0, 109.0, 594.0, 24.0));
        opacity_slider.setMinValue(0.0);
        opacity_slider.setMaxValue(100.0);
        opacity_slider.setContinuous(true);
        opacity_slider.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "背景不透明度",
            "Background opacity"
        ))));
        let opacity_input =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(634.0, 107.0, 58.0, 26.0));
        opacity_input.setAlignment(NSTextAlignment::Right);
        opacity_input.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "不透明度百分比",
            "Opacity percentage"
        ))));
        set_action(&opacity_slider, target, sel!(changeBackgroundOpacity:));
        set_action(&opacity_input, target, sel!(commitBackgroundOpacity:));
        opacity_slider.setEnabled(!glass);
        opacity_input.setEnabled(!glass);
        opacity.addSubview(&opacity_slider);
        opacity.addSubview(&opacity_input);
        opacity.addSubview(&label("%", 13.0, rect(700.0, 109.0, 20.0, 24.0), mtm));
        let sample = NSView::initWithFrame(NSView::alloc(mtm), rect(20.0, 20.0, 700.0, 64.0));
        for (x, color) in [
            (0.0, NSColor::systemIndigoColor()),
            (350.0, NSColor::systemTealColor()),
        ] {
            let tile = NSBox::initWithFrame(NSBox::alloc(mtm), rect(x, 0.0, 350.0, 64.0));
            tile.setBoxType(NSBoxType::Custom);
            tile.setBorderWidth(0.0);
            tile.setFillColor(&color.colorWithAlphaComponent(0.35));
            sample.addSubview(&tile);
        }
        let opacity_preview = crate::app::PanelBackdrop::new(sample.bounds(), mtm);
        opacity_preview.blend_within_window();
        sample.addSubview(opacity_preview.view());
        opacity_preview.content.addSubview(&label(
            tr!(
                "预览 · 搜索和切换面板",
                "Preview · Search and switch panels"
            ),
            14.0,
            rect(18.0, 20.0, 665.0, 24.0),
            mtm,
        ));
        opacity.addSubview(&sample);

        let source = settings_group(
            &input_tab,
            tr!("搜索输入法", "Search input source"),
            570.0,
            112.0,
            mtm,
        );
        source.addSubview(&label(
            tr!("打开搜索时使用", "When search opens"),
            14.0,
            rect(20.0, 72.0, 285.0, 24.0),
            mtm,
        ));
        let input_method = popup(
            &[
                tr!("跟随当前输入法", "Keep current input source"),
                tr!("始终英文", "Always English"),
                tr!("始终中文", "Always Chinese"),
                tr!("记住 Winlane 上次使用", "Last used in Winlane"),
            ],
            rect(330.0, 69.0, 390.0, 28.0),
            mtm,
        );
        input_method.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "搜索输入法",
            "Search input method"
        ))));
        source.addSubview(&input_method);
        source.addSubview(&hint(tr!("从切换模式按 Space 进入搜索时也会应用；输入过程中仍可手动切换。", "Also applies when Space opens search from switch mode. You can still change input sources while typing."), rect(20.0, 14.0, 700.0, 44.0), mtm));
        let behavior = settings_group(
            &input_tab,
            tr!("输入行为", "How it works"),
            404.0,
            236.0,
            mtm,
        );
        for (title, description, top) in [
            (
                tr!("英文 / 中文", "English / Chinese"),
                tr!(
                    "使用 macOS 对应语言的已启用输入法，支持第三方输入法。",
                    "Use the enabled input source macOS chooses for the language, including third-party input methods."
                ),
                234.0,
            ),
            (
                tr!("记住上次使用", "Last used in Winlane"),
                tr!(
                    "记住上次在 Winlane 中使用的输入法，重启后也会保留。",
                    "Remember the last input source used in Winlane, even after restarting."
                ),
                157.0,
            ),
            (
                tr!("输入法不可用时", "If a source is unavailable"),
                tr!(
                    "保留当前输入法。切换模式中的 alias 不受影响。",
                    "Keep the current input source. Switch-mode aliases are unaffected."
                ),
                80.0,
            ),
        ] {
            row_text(&behavior, title, description, top, 700.0, mtm);
        }
        row_divider(&behavior, 158.0, mtm);
        row_divider(&behavior, 81.0, mtm);

        let listing = settings_group(&windows, tr!("窗口列表", "Window list"), 570.0, 130.0, mtm);
        listing.addSubview(&label(
            tr!("窗口排序", "Sort windows"),
            14.0,
            rect(20.0, 91.0, 315.0, 24.0),
            mtm,
        ));
        let sort = popup(
            &[
                tr!("最近在 Winlane 中切换", "Recently switched in Winlane"),
                tr!("应用名称", "Application name"),
                tr!("窗口标题", "Window title"),
            ],
            rect(375.0, 88.0, 345.0, 28.0),
            mtm,
        );
        listing.addSubview(&sort);
        row_divider(&listing, 67.0, mtm);
        let minimized = checkbox(
            tr!("在列表中显示最小化窗口", "Include minimized windows"),
            mtm,
        );
        minimized.setFrame(rect(20.0, 23.0, 690.0, 26.0));
        listing.addSubview(&minimized);
        let exclusions = settings_group(
            &windows,
            tr!("排除的应用", "Excluded apps"),
            386.0,
            124.0,
            mtm,
        );
        let excluded =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(20.0, 71.0, 700.0, 28.0));
        excluded.setPlaceholderString(Some(&NSString::from_str(tr!(
            "例如：Finder, Terminal",
            "For example: Finder, Terminal"
        ))));
        exclusions.addSubview(&excluded);
        exclusions.addSubview(&hint(tr!("填写列表中显示的完整应用名，用逗号分隔。这些应用将不出现在候选列表里。", "Enter app names exactly as shown in the list, separated by commas. These apps will be hidden from results."), rect(20.0, 18.0, 700.0, 42.0), mtm));
        let timing = settings_group(&windows, tr!("响应速度", "Timing"), 208.0, 102.0, mtm);
        row_text(
            &timing,
            tr!("切换面板显示延迟（毫秒）", "Switcher display delay (ms)"),
            tr!(
                "快速松键直接切换；0 表示立即显示。",
                "Quick releases switch directly; 0 shows the panel immediately."
            ),
            89.0,
            505.0,
            mtm,
        );
        let switch_delay =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(560.0, 38.0, 160.0, 28.0));
        timing.addSubview(&switch_delay);
        for field in [&excluded, &switch_delay, &opacity_input] {
            field.cell().unwrap().setSendsActionOnEndEditing(true);
        }
        let controls: [&NSControl; 8] = [
            &*language,
            &*appearance,
            &*density,
            &*sort,
            &*input_method,
            &*excluded,
            &*switch_delay,
            &*minimized,
        ];
        for control in controls {
            set_action(control, target, sel!(settingsChanged:));
        }
        search_shortcut.on_change(target, sel!(settingsChanged:));
        switch_shortcut.on_change(target, sel!(settingsChanged:));

        let alias_card = settings_group(
            &aliases,
            tr!("应用与项目", "Apps & projects"),
            570.0,
            172.0,
            mtm,
        );
        alias_card.addSubview(&label(
            tr!("固定常用窗口的字母", "Keep familiar aliases"),
            16.0,
            rect(20.0, 130.0, 700.0, 25.0),
            mtm,
        ));
        alias_card.addSubview(&hint(tr!("例如 w → WeChat，ck → Code 的 ckb 项目。\n规则优先于自动分配，重启后保留；标题关键词不区分大小写。", "For example, w → WeChat, ck → the ckb project in Code.\nRules override automatic aliases and survive restarts. Title matching ignores case."), rect(20.0, 68.0, 700.0, 50.0), mtm));
        alias_card.addSubview(&button(
            tr!("配置 Alias 规则…", "Alias Rules…"),
            target,
            sel!(showAliasRules:),
            rect(500.0, 20.0, 220.0, 30.0),
            mtm,
        ));
        let clipboard =
            crate::clipboard_settings::ClipboardControls::new(&clipboard_tab, target, mtm);
        let message = hint("", rect(252.0, 4.0, 740.0, 38.0), mtm);
        view.addSubview(&message);
        // SAFETY: The application delegate outlives the settings window and handles section changes.
        unsafe {
            let _: () = msg_send![&tabs, setDelegate: target];
        }
        let settings = Self {
            window,
            tabs,
            navigation,
            page_title,
            page_description,
            search_card,
            switch_card,
            app_shortcuts_card,
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
        };
        settings.select_tab(4);
        settings
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
            .max(search_height + 208.0);
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
    pub fn set_quicklinks(&self, links: &[winlane::quicklinks::Quicklink]) {
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
                "选择每次进入搜索时使用的输入法。",
                "Choose the input source used when you start a search."
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

fn settings_tab(tabs: &NSTabView, title: &str, mtm: MainThreadMarker) -> Retained<NSView> {
    let item = NSTabViewItem::new();
    item.setLabel(&NSString::from_str(title));
    let view = NSView::initWithFrame(NSView::alloc(mtm), tabs.contentRect());
    item.setView(Some(&view));
    tabs.addTabViewItem(&item);
    view
}

pub(crate) fn card(parent: &NSView, frame: NSRect, mtm: MainThreadMarker) -> Retained<NSView> {
    let view = NSView::initWithFrame(NSView::alloc(mtm), frame);
    let background = NSBox::initWithFrame(NSBox::alloc(mtm), view.bounds());
    background.setBoxType(NSBoxType::Custom);
    background.setTitlePosition(NSTitlePosition::NoTitle);
    background.setCornerRadius(10.0);
    background.setBorderWidth(1.0);
    background.setBorderColor(&NSColor::separatorColor().colorWithAlphaComponent(0.5));
    background.setFillColor(&NSColor::controlBackgroundColor().colorWithAlphaComponent(0.5));
    background.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    view.addSubview(&background);
    parent.addSubview(&view);
    view
}

pub(crate) fn settings_group(
    parent: &NSView,
    title: &str,
    top: f64,
    height: f64,
    mtm: MainThreadMarker,
) -> Retained<NSView> {
    let heading = hint(title, rect(6.0, top - 22.0, 728.0, 22.0), mtm);
    heading.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
    parent.addSubview(&heading);
    card(parent, rect(0.0, top - 32.0 - height, 740.0, height), mtm)
}

pub(crate) fn row_text(
    parent: &NSView,
    title: &str,
    description: &str,
    top: f64,
    width: f64,
    mtm: MainThreadMarker,
) {
    parent.addSubview(&label(
        title,
        14.0,
        rect(20.0, top - 33.0, width, 24.0),
        mtm,
    ));
    parent.addSubview(&hint(description, rect(20.0, top - 72.0, width, 38.0), mtm));
}

pub(crate) fn editor_chrome(parent: &NSView, mtm: MainThreadMarker) {
    for frame in [rect(0.0, 0.0, 204.0, 574.0), rect(220.0, 0.0, 520.0, 574.0)] {
        let background = card(parent, frame, mtm);
        parent.addSubview_positioned_relativeTo(&background, NSWindowOrderingMode::Below, None);
    }
    let heading = hint(
        tr!("内容列表", "LIBRARY"),
        rect(16.0, 538.0, 174.0, 22.0),
        mtm,
    );
    heading.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
    parent.addSubview(&heading);
}

pub(crate) fn row_divider(parent: &NSView, y: f64, mtm: MainThreadMarker) {
    let line = NSBox::initWithFrame(
        NSBox::alloc(mtm),
        rect(20.0, y, parent.bounds().size.width - 40.0, 1.0),
    );
    line.setBoxType(NSBoxType::Separator);
    parent.addSubview(&line);
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
    let value = NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm);
    value.setFont(Some(&NSFont::systemFontOfSize(12.0)));
    value.setFrame(frame);
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
