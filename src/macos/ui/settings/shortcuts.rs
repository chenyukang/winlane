use super::*;

pub(super) struct ShortcutsPage {
    pub(super) search_shortcut: ShortcutControls,
    pub(super) search_rows: RefCell<Vec<SearchShortcutRow>>,
    pub(super) shortcuts_document: Retained<NSView>,
    pub(super) shortcuts_scroll: Retained<NSScrollView>,
    pub(super) search_header: Retained<NSView>,
    pub(super) add_search: Retained<NSButton>,
    pub(super) switch_shortcut: ShortcutControls,
    pub(super) search_card: Retained<NSView>,
    pub(super) switch_card: Retained<NSView>,
    pub(super) app_shortcuts_card: Retained<NSView>,
    pub(super) command_shortcuts: command_shortcuts::CommandShortcutControls,
}

impl ShortcutsPage {
    pub(super) fn new(shortcuts_host: &NSView, target: &AnyObject, mtm: MainThreadMarker) -> Self {
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

        let command_shortcuts =
            command_shortcuts::CommandShortcutControls::new(&shortcuts, target, mtm);

        search_shortcut.on_change(target, sel!(settingsChanged:));
        switch_shortcut.on_change(target, sel!(settingsChanged:));
        Self {
            search_shortcut,
            search_rows: RefCell::default(),
            shortcuts_document: shortcuts,
            shortcuts_scroll,
            search_header,
            add_search,
            switch_shortcut,
            search_card,
            switch_card,
            app_shortcuts_card,
            command_shortcuts,
        }
    }
    pub(super) fn fill(&self, config: &Config) {
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
        self.command_shortcuts.fill(&config.command_shortcuts);
    }
    pub(super) fn read(&self, config: &mut Config) -> Result<(), String> {
        config.shortcut = self.search_shortcut.read()?;
        config.additional_search_shortcuts = self
            .search_rows
            .borrow()
            .iter()
            .map(|row| row.shortcut.read())
            .collect::<Result<_, _>>()?;
        config.switch_shortcut = self.switch_shortcut.read()?;
        config.command_shortcuts = self.command_shortcuts.read()?;
        Ok(())
    }
    pub(super) fn append_search_row(&self, value: &Shortcut) {
        let mtm = self.search_card.mtm();
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
    pub(super) fn layout_search_shortcuts(&self) {
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
}
