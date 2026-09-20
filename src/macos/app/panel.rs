use super::*;

impl Delegate {
    pub(super) fn panels(&self) -> Vec<Rc<PanelUi>> {
        self.ivars().panels.borrow().clone()
    }

    pub(super) fn any_panel_visible(&self) -> bool {
        self.panels().iter().any(|ui| ui.panel.isVisible())
    }

    pub(super) fn any_panel_key(&self) -> bool {
        self.panels().iter().any(|ui| ui.panel.isKeyWindow())
    }

    pub(super) fn sync_displays(&self) {
        let state = self.ivars();
        state.changing_displays.set(true);
        let syncing_controls = state.syncing_controls.replace(true);
        let existing = self.panels();
        let focused = existing
            .iter()
            .find(|ui| ui.panel.isKeyWindow())
            .map(|ui| ui.display_id);
        let displays: Vec<_> = NSScreen::screens(self.mtm())
            .iter()
            .enumerate()
            .map(|(index, screen)| {
                let id = screen
                    .deviceDescription()
                    .objectForKey(ns_string!("NSScreenNumber"))
                    .and_then(|value| value.downcast::<NSNumber>().ok())
                    .map_or(index as u32, |number| number.unsignedIntValue());
                Display {
                    id,
                    frame: display_rect(screen.frame()),
                    visible: display_rect(screen.visibleFrame()),
                }
            })
            .collect();
        let pointer = NSEvent::mouseLocation();
        let positions = placements(&displays, (WIDTH, HEIGHT), (pointer.x, pointer.y), focused);
        state.keyboard_display.set(
            positions
                .iter()
                .find(|position| position.receives_keyboard)
                .map(|position| position.display_id),
        );
        let mut panels = Vec::new();
        for position in positions {
            let ui = existing
                .iter()
                .find(|ui| ui.display_id == position.display_id)
                .cloned()
                .unwrap_or_else(|| self.create_panel(position.display_id));
            if state.mode.get().is_some() {
                self.render_panel(&ui);
            }
            let display = displays
                .iter()
                .find(|display| display.id == position.display_id)
                .unwrap();
            let y = display.visible.y
                + ((display.visible.height - ui.panel.frame().size.height) * 0.58).max(0.0);
            ui.panel.setFrameOrigin(NSPoint::new(position.x, y));
            panels.push(ui);
        }
        state.panels.replace(panels.clone());
        for ui in existing
            .iter()
            .filter(|ui| !panels.iter().any(|new| new.display_id == ui.display_id))
        {
            ui.panel.setDelegate(None);
            ui.panel.orderOut(None);
            ui.panel.close();
        }
        state.syncing_controls.set(syncing_controls);
        state.changing_displays.set(false);
    }

    pub(super) fn present_panels(&self) {
        self.cancel_switch_timer();
        let state = self.ivars();
        state.changing_displays.set(true);
        let panels = self.panels();
        for ui in &panels {
            ui.panel.orderFrontRegardless();
        }
        if let Some(ui) = panels
            .iter()
            .find(|ui| Some(ui.display_id) == state.keyboard_display.get())
        {
            ui.panel.makeKeyAndOrderFront(None);
            if state.mode.get() == Some(PanelMode::Switch) {
                ui.panel.makeFirstResponder(None);
            } else {
                self.focus_search();
            }
        }
        state.changing_displays.set(false);
        self.complete_input_start();
    }

    pub(super) fn create_panel(&self, display_id: u32) -> Rc<PanelUi> {
        let mtm = self.mtm();
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
        panel.setTitleVisibility(NSWindowTitleVisibility::Hidden);
        panel.setTitlebarAppearsTransparent(true);
        for button in [
            NSWindowButton::CloseButton,
            NSWindowButton::MiniaturizeButton,
            NSWindowButton::ZoomButton,
        ] {
            if let Some(button) = panel.standardWindowButton(button) {
                button.setHidden(true);
            }
        }
        panel.setLevel(NSFloatingWindowLevel);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::IgnoresCycle
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        panel.setDelegate(Some(ProtocolObject::from_ref(self)));
        panel.setHidesOnDeactivate(false);
        panel.setBecomesKeyOnlyIfNeeded(false);
        panel.setMovableByWindowBackground(true);
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        let root: Retained<PanelContentView> = unsafe {
            msg_send![super(PanelContentView::alloc(mtm).set_ivars(())),
                initWithFrame: rect(0.0, 0.0, WIDTH, HEIGHT)]
        };
        panel.setContentView(Some(&root));
        panel.setInitialFirstResponder(Some(&root));
        let backdrop = PanelBackdrop::new(root.bounds(), mtm);
        root.addSubview(backdrop.view());
        let root = &backdrop.content;

        let shortcut = label(
            &self.ivars().config.borrow().shortcut.display(),
            11.0,
            rect(WIDTH - 120.0, 546.0, 104.0, 18.0),
            mtm,
        );
        shortcut.setAlignment(NSTextAlignment::Right);
        shortcut.setTextColor(Some(&NSColor::labelColor()));
        shortcut.setAlphaValue(0.65);
        root.addSubview(&shortcut);

        let project_progress = NSProgressIndicator::initWithFrame(
            NSProgressIndicator::alloc(mtm),
            rect(WIDTH - 36.0, 546.0, 16.0, 16.0),
        );
        project_progress.setStyle(NSProgressIndicatorStyle::Spinning);
        project_progress.setControlSize(NSControlSize::Small);
        project_progress.setIndeterminate(true);
        project_progress.setDisplayedWhenStopped(false);
        project_progress.setHidden(true);
        project_progress.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "正在更新项目",
            "Updating projects"
        ))));
        root.addSubview(&project_progress);

        let input_width = ((WIDTH - 32.0) * 0.618).round();
        let input = NSSearchField::initWithFrame(
            NSSearchField::alloc(mtm),
            rect((WIDTH - input_width) / 2.0, 538.0, input_width, 34.0),
        );
        input.setFont(Some(&NSFont::systemFontOfSize(15.0)));
        input.setFocusRingType(NSFocusRingType::None);
        input.setPlaceholderString(Some(ns_string!("")));
        input.setSendsSearchStringImmediately(true);
        input.setMaximumRecents(0);
        self.configure_input_start(&input);
        unsafe {
            input.setDelegate(Some(ProtocolObject::from_ref(self)));
            root.addSubview(&input);
        }
        let scope_back = self.button(
            tr!("‹ 片段", "‹ Snippets"),
            sel!(leaveScopedSearch:),
            rect(10.0, 538.0, 110.0, 34.0),
        );
        scope_back.setBordered(false);
        scope_back.setHidden(true);
        scope_back.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "返回窗口搜索",
            "Back to window search"
        ))));
        root.addSubview(&scope_back);
        let clipboard_actions = NSPopUpButton::initWithFrame_pullsDown(
            NSPopUpButton::alloc(mtm),
            rect(WIDTH - 128.0, 538.0, 118.0, 30.0),
            false,
        );
        for title in [
            tr!("操作…", "Actions…"),
            tr!("复制  ⌘C", "Copy  ⌘C"),
            tr!("删除  ⌘⌫", "Delete  ⌘⌫"),
            tr!("暂停记录", "Pause recording"),
            tr!("清空历史…", "Clear history…"),
        ] {
            clipboard_actions.addItemWithTitle(&NSString::from_str(title));
        }
        unsafe {
            clipboard_actions.setTarget(Some(self));
            clipboard_actions.setAction(Some(sel!(clipboardActions:)));
        }
        clipboard_actions.setHidden(true);
        root.addSubview(&clipboard_actions);
        let quicklink_bar = crate::macos::ui::quicklinks::input::Bar::new(self, mtm);
        root.addSubview(&quicklink_bar.view);
        let mode_label = label("", 11.0, rect(16.0, LIST_BOTTOM, WIDTH - 32.0, 20.0), mtm);
        mode_label.setTextColor(Some(&NSColor::labelColor()));
        mode_label.setAlphaValue(0.65);
        mode_label.setHidden(true);
        root.addSubview(&mode_label);

        let scroll = NSScrollView::initWithFrame(
            NSScrollView::alloc(mtm),
            rect(10.0, LIST_BOTTOM, LIST_WIDTH, LIST_TOP - LIST_BOTTOM),
        );
        scroll.setHasVerticalScroller(false);
        scroll.setDrawsBackground(false);
        scroll.setBorderType(NSBorderType::NoBorder);
        let list: Retained<ListView> = unsafe {
            msg_send![ListView::alloc(mtm), initWithFrame: rect(0.0, 0.0, LIST_WIDTH, LIST_TOP - LIST_BOTTOM)]
        };
        scroll.setDocumentView(Some(&list));
        root.addSubview(&scroll);

        let footer = label(
            tr!("正在准备窗口列表…", "Preparing windows…"),
            11.0,
            rect(16.0, 9.0, WIDTH - 148.0, 18.0),
            mtm,
        );
        footer.setTextColor(Some(&NSColor::labelColor()));
        footer.setAlphaValue(0.65);
        root.addSubview(&footer);
        let help = self.button(
            tr!("辅助功能设置…", "Accessibility Settings…"),
            sel!(openPermissions:),
            rect(16.0, 34.0, 180.0, 25.0),
        );
        root.addSubview(&help);
        let demo_button = self.button(
            tr!("查看演示", "View Demo"),
            sel!(toggleDemo:),
            rect(204.0, 34.0, 142.0, 25.0),
        );
        root.addSubview(&demo_button);
        let refresh = self.button(
            tr!("刷新 ↻", "Refresh ↻"),
            sel!(refreshWindows:),
            rect(WIDTH - 106.0, 34.0, 90.0, 25.0),
        );
        refresh.setHidden(accessibility::is_trusted());
        root.addSubview(&refresh);
        let settings_button = self.button(
            tr!("设置…  ⌘,", "Settings…  ⌘,"),
            sel!(showSettings:),
            rect(WIDTH - 126.0, 9.0, 110.0, 24.0),
        );
        settings_button.setBordered(false);
        settings_button.setContentTintColor(Some(&NSColor::labelColor()));
        root.addSubview(&settings_button);
        let history_permissions_button = self.button(
            tr!("打开访问权限设置…", "Open Access Settings…"),
            sel!(openHistoryPermissions:),
            rect(WIDTH - 206.0, 9.0, 190.0, 24.0),
        );
        history_permissions_button.setHidden(true);
        root.addSubview(&history_permissions_button);

        Rc::new(PanelUi {
            file_preview: RefCell::new(None),
            project_progress,
            display_id,
            panel,
            backdrop,
            input,
            scroll,
            list,
            footer,
            help,
            demo_button,
            refresh_button: refresh,
            settings_button,
            history_permissions_button,
            scope_back,
            clipboard_actions,
            quicklink_bar: RefCell::new(quicklink_bar),
            shortcut_label: shortcut,
            mode_label,
            rows: RefCell::new(Vec::new()),
            empty_labels: RefCell::new(Vec::new()),
        })
    }

    pub(super) fn button(&self, title: &str, action: Sel, frame: NSRect) -> Retained<NSButton> {
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
}

pub(super) fn display_rect(frame: NSRect) -> Rect {
    Rect {
        x: frame.origin.x,
        y: frame.origin.y,
        width: frame.size.width,
        height: frame.size.height,
    }
}
