use super::*;

impl Delegate {
    pub(super) fn build_menus(&self) {
        let mtm = self.mtm();
        let app = NSApplication::sharedApplication(mtm);
        let main_menu = NSMenu::new(mtm);
        let application_item = NSMenuItem::new(mtm);
        let application_menu = NSMenu::new(mtm);
        // SAFETY: quitApp: is implemented by this retained delegate.
        let quit_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(tr!("退出 Winlane", "Quit Winlane")),
                Some(sel!(quitApp:)),
                ns_string!("q"),
            )
        };
        unsafe { quit_item.setTarget(Some(self)) };
        application_menu.addItem(&self.menu_item(
            tr!("检查更新…", "Check for Updates…"),
            sel!(checkForUpdates:),
            "",
        ));
        application_menu.addItem(&self.menu_item(
            tr!("反馈…", "Feedback…"),
            sel!(openFeedback:),
            "",
        ));
        application_menu.addItem(&quit_item);
        let settings_item = self.menu_item(tr!("设置…", "Settings…"), sel!(showSettings:), ",");
        application_menu.insertItem_atIndex(&settings_item, 0);
        application_menu.insertItem_atIndex(
            &self.menu_item(
                tr!("打开窗口搜索", "Open Window Search"),
                sel!(showSearch:),
                "",
            ),
            0,
        );
        application_item.setSubmenu(Some(&application_menu));
        main_menu.addItem(&application_item);
        let edit_menu = NSMenu::new(mtm);
        let edit_item = NSMenuItem::new(mtm);
        edit_item.setTitle(&NSString::from_str(tr!("编辑", "Edit")));
        for (title, action, key) in [
            (tr!("剪切", "Cut"), sel!(cut:), "x"),
            (tr!("拷贝", "Copy"), sel!(copy:), "c"),
            (tr!("粘贴", "Paste"), sel!(paste:), "v"),
            (tr!("全选", "Select All"), sel!(selectAll:), "a"),
        ] {
            // SAFETY: Standard editing actions are resolved by AppKit's responder chain.
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(title),
                    Some(action),
                    &NSString::from_str(key),
                )
            };
            edit_menu.addItem(&item);
        }
        edit_item.setSubmenu(Some(&edit_menu));
        main_menu.addItem(&edit_item);
        let window_item = NSMenuItem::new(mtm);
        window_item.setTitle(&NSString::from_str(tr!("窗口", "Window")));
        let window_menu = NSMenu::new(mtm);
        window_menu.addItem(&self.menu_item(
            tr!("关闭窗口", "Close Window"),
            sel!(closeWindow:),
            "w",
        ));
        window_menu.addItem(&NSMenuItem::separatorItem(mtm));
        window_menu.addItem(&self.menu_item(
            tr!("仅当前应用", "Current app only"),
            sel!(toggleScope:),
            "",
        ));
        window_menu.addItem(&NSMenuItem::separatorItem(mtm));
        self.add_window_actions(&window_menu);
        window_menu.addItem(&NSMenuItem::separatorItem(mtm));
        window_menu.addItem(&self.menu_item(
            tr!("刷新窗口", "Refresh Windows"),
            sel!(refreshWindows:),
            "r",
        ));
        for index in 0..9 {
            let item = self.menu_item(
                &trf!("切换到第 {} 个窗口", "Switch to Window {}", index + 1),
                sel!(quickSelect:),
                &(index + 1).to_string(),
            );
            item.setTag(index);
            window_menu.addItem(&item);
        }
        window_item.setSubmenu(Some(&window_menu));
        main_menu.addItem(&window_item);
        app.setMainMenu(Some(&main_menu));

        let status_item = self.ivars().status_item.get_or_init(|| {
            NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength)
        });
        if let Some(button) = status_item.button(mtm) {
            let data =
                NSData::with_bytes(include_bytes!("../../../resources/MenuBarIconTemplate.pdf"));
            let icon = NSImage::initWithData(NSImage::alloc(), &data)
                .expect("the embedded menu-bar icon must be a valid PDF");
            icon.setSize(NSSize::new(18.0, 18.0));
            icon.setTemplate(true);
            button.setTitle(ns_string!(""));
            button.setImage(Some(&icon));
            button.setImagePosition(NSCellImagePosition::ImageOnly);
            button.setAccessibilityLabel(Some(ns_string!("Winlane")));
            button.setToolTip(Some(&NSString::from_str(tr!(
                "Winlane · 窗口搜索",
                "Winlane · Window Search"
            ))));
        }
        let menu = NSMenu::new(mtm);
        for (title, action, key) in [
            (
                tr!("打开窗口搜索", "Open Window Search"),
                sel!(showSearch:),
                "",
            ),
            (
                tr!("打开窗口切换", "Open Window Switcher"),
                sel!(showSwitcher:),
                "",
            ),
            (tr!("设置…", "Settings…"), sel!(showSettings:), ","),
            (
                tr!("刷新窗口列表", "Refresh Window List"),
                sel!(refreshWindows:),
                "",
            ),
            (
                tr!("辅助功能设置…", "Accessibility Settings…"),
                sel!(openPermissions:),
                "",
            ),
            (tr!("反馈…", "Feedback…"), sel!(openFeedback:), ""),
            (
                tr!("检查更新…", "Check for Updates…"),
                sel!(checkForUpdates:),
                "",
            ),
            (tr!("退出 Winlane", "Quit Winlane"), sel!(quitApp:), "q"),
        ] {
            // SAFETY: These selectors belong to this retained delegate and accept one object argument.
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(title),
                    Some(action),
                    &NSString::from_str(key),
                )
            };
            unsafe { item.setTarget(Some(self)) };
            menu.addItem(&item);
            if action == sel!(showSearch:) {
                self.ivars().show_menu_item.replace(Some(item));
            } else if action == sel!(showSwitcher:) {
                self.ivars().switch_menu_item.replace(Some(item));
            }
        }
        status_item.setMenu(Some(&menu));
        self.update_shortcut_labels();
    }

    pub(super) fn update_shortcut_labels(&self) {
        let config = self.ivars().config.borrow();
        for (item, title, shortcut) in [
            (
                &self.ivars().show_menu_item,
                tr!("打开窗口搜索", "Open Window Search"),
                config.search_shortcuts_display(),
            ),
            (
                &self.ivars().switch_menu_item,
                tr!("打开窗口切换", "Open Window Switcher"),
                config.switch_shortcut.display(),
            ),
        ] {
            if let Some(item) = item.borrow().as_ref() {
                item.setTitle(&NSString::from_str(&format!("{title}    {}", shortcut)));
            }
        }
    }

    pub(super) fn menu_item(&self, title: &str, action: Sel, key: &str) -> Retained<NSMenuItem> {
        // SAFETY: This retained delegate implements the selectors used by these menu items.
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(self.mtm()),
                &NSString::from_str(title),
                Some(action),
                &NSString::from_str(key),
            )
        };
        unsafe { item.setTarget(Some(self)) };
        item
    }

    pub(super) fn add_window_actions(&self, menu: &NSMenu) {
        menu.addItem(&self.menu_item(
            tr!(
                "最小化 / 恢复所选窗口",
                "Minimize / Restore Selected Window"
            ),
            sel!(minimizeChosen:),
            "m",
        ));
        menu.addItem(&self.menu_item(
            tr!("隐藏所选应用", "Hide Selected App"),
            sel!(hideChosen:),
            "h",
        ));
        let copy = self.menu_item(
            tr!("复制窗口标题", "Copy Window Title"),
            sel!(copyTitle:),
            "c",
        );
        copy.setKeyEquivalentModifierMask(
            NSEventModifierFlags::Command | NSEventModifierFlags::Shift,
        );
        menu.addItem(&copy);
    }
}
