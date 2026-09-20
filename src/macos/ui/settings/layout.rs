use super::*;

impl SettingsWindow {
    pub fn new(target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let window = preferences_window(rect(0.0, 0.0, 1020.0, 740.0), mtm);
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::MoveToActiveSpace
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Auxiliary,
        );
        window.center();
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
            rect(24.0, 270.0, 178.0, 20.0),
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
        settings_tab(&tabs, tr!("快捷键", "Shortcuts"), mtm);
        settings_tab(&tabs, tr!("外观", "Appearance"), mtm);
        settings_tab(&tabs, tr!("输入法指示器", "Input Indicator"), mtm);
        settings_tab(&tabs, tr!("窗口列表", "Windows"), mtm);
        settings_tab(&tabs, tr!("常规", "General"), mtm);
        settings_tab(&tabs, tr!("Alias 规则", "Aliases"), mtm);
        settings_tab(&tabs, tr!("文本片段", "Snippets"), mtm);
        settings_tab(&tabs, tr!("剪贴板", "Clipboard"), mtm);
        settings_tab(&tabs, tr!("快捷链接", "Quicklinks"), mtm);
        settings_tab(&tabs, tr!("输入法规则", "Input Rules"), mtm);
        settings_tab(&tabs, tr!("鼠标滚轮", "Mouse Scrolling"), mtm);
        let mut navigation = Vec::new();
        for (position, (index, symbol, color)) in [
            (4, "gearshape.fill", NSColor::systemGrayColor()),
            (1, "paintpalette.fill", NSColor::systemPinkColor()),
            (0, "keyboard", NSColor::systemPurpleColor()),
            (9, "character.cursor.ibeam", NSColor::systemBlueColor()),
            (2, "circle.fill", NSColor::systemCyanColor()),
            (3, "macwindow.on.rectangle", NSColor::systemIndigoColor()),
            (5, "textformat.abc", NSColor::systemTealColor()),
            (6, "text.quote", NSColor::systemGreenColor()),
            (7, "doc.on.clipboard", NSColor::systemOrangeColor()),
            (8, "link", NSColor::systemCyanColor()),
            (10, "computermouse.fill", NSColor::systemBlueColor()),
        ]
        .into_iter()
        .enumerate()
        {
            let y = if position < 7 {
                568.0 - position as f64 * 44.0
            } else {
                220.0 - (position - 7) as f64 * 44.0
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

        let message = hint("", rect(252.0, 4.0, 740.0, 38.0), mtm);
        view.addSubview(&message);
        tabs.selectTabViewItemAtIndex(4);
        // SAFETY: The delegate receives tab changes on the main thread.
        unsafe {
            let _: () = msg_send![&tabs, setDelegate: target];
        }
        Self {
            window,
            tabs,
            navigation,
            page_title,
            page_description,
            message,
            target: Weak::new(target),
            config: RefCell::new(Config::default()),
            shortcuts: OnceCell::new(),
            appearance: OnceCell::new(),
            general: OnceCell::new(),
            input: OnceCell::new(),
            input_rules: OnceCell::new(),
            scrolling: OnceCell::new(),
            scroll_status: RefCell::new((String::new(), false)),
            windows: OnceCell::new(),
            clipboard: OnceCell::new(),
            updater: RefCell::new((false, false, None)),
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
