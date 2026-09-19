use super::*;

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
        let glass = crate::macos::ui::material::glass_available();
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
        let opacity_preview = crate::macos::ui::material::PanelBackdrop::new(sample.bounds(), mtm);
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
        let clipboard = crate::macos::ui::clipboard_settings::ClipboardControls::new(
            &clipboard_tab,
            target,
            mtm,
        );
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
}

fn settings_tab(tabs: &NSTabView, title: &str, mtm: MainThreadMarker) -> Retained<NSView> {
    let item = NSTabViewItem::new();
    item.setLabel(&NSString::from_str(title));
    let view = NSView::initWithFrame(NSView::alloc(mtm), tabs.contentRect());
    item.setView(Some(&view));
    tabs.addTabViewItem(&item);
    view
}
