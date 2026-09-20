use super::*;

#[derive(Clone, Copy)]
pub(super) enum Action {
    Open,
    Browse,
    Reveal,
    Preview,
    Copy,
    CopyPath,
}

impl Delegate {
    pub(in crate::macos::app) fn file_actions_menu(&self) -> Retained<NSMenu> {
        let menu = NSMenu::new(self.mtm());
        menu.setAutoenablesItems(false);
        let Some(entry) = self.selected_file() else {
            return menu;
        };
        let snapshot =
            NSString::from_str(&serde_json::to_string(&entry).expect("file entry serializes"));
        let command = NSEventModifierFlags::Command;
        let mut items = Vec::new();
        if entry.directory {
            items.push((
                Action::Browse,
                tr!("进入目录", "Browse Folder"),
                "\r",
                NSEventModifierFlags::empty(),
            ));
        }
        items.push((
            Action::Open,
            if entry.directory {
                tr!("在 Finder 中打开", "Open in Finder")
            } else {
                tr!("打开文件", "Open File")
            },
            "\r",
            if entry.directory {
                NSEventModifierFlags::Control
            } else {
                NSEventModifierFlags::empty()
            },
        ));
        items.extend([
            (
                Action::Reveal,
                tr!("在 Finder 中显示", "Reveal in Finder"),
                "\r",
                command,
            ),
            (
                Action::Preview,
                tr!("预览 / 关闭预览", "Toggle Preview"),
                "y",
                command,
            ),
            (Action::Copy, tr!("复制文件", "Copy File"), "c", command),
            (
                Action::CopyPath,
                tr!("复制完整路径", "Copy Full Path"),
                "c",
                command | NSEventModifierFlags::Shift,
            ),
        ]);
        for (action, title, key, modifiers) in items {
            let item = self.menu_item(title, sel!(performFileAction:), key);
            item.setTag(action as isize);
            item.setKeyEquivalentModifierMask(modifiers);
            unsafe {
                item.setRepresentedObject(Some(&snapshot));
            }
            menu.addItem(&item);
        }
        menu
    }

    pub(in crate::macos::app) fn show_file_actions(&self) {
        if !self.searching_files() || self.selected_file().is_none() {
            return;
        }
        let panels = self.panels();
        let Some(ui) = panels.iter().find(|ui| ui.panel.isKeyWindow()) else {
            return;
        };
        let menu = self.file_actions_menu();
        self.ivars().files.borrow_mut().menu_open = true;
        menu.popUpMenuPositioningItem_atLocation_inView(
            None,
            NSPoint::new(0.0, 0.0),
            Some(&ui.file_controls.actions),
        );
        self.ivars().files.borrow_mut().menu_open = false;
        self.ivars().check_panel_focus.set(true);
        self.focus_search();
    }

    pub(in crate::macos::app) fn file_menu_action(&self, item: &NSMenuItem) {
        let Some(entry) = item
            .representedObject()
            .and_then(|value| value.downcast::<NSString>().ok())
            .and_then(|text| serde_json::from_str::<Entry>(&text.to_string()).ok())
        else {
            return;
        };
        let action = match item.tag() {
            0 => Action::Open,
            1 => Action::Browse,
            2 => Action::Reveal,
            3 => Action::Preview,
            4 => Action::Copy,
            5 => Action::CopyPath,
            _ => return,
        };
        self.perform_file_action(action, entry);
    }

    pub(super) fn perform_file_action(&self, action: Action, entry: Entry) {
        if !self.searching_files() {
            return;
        }
        match action {
            Action::Browse => {
                self.complete_file_entry(&entry);
            }
            Action::Open => self.open_file_entry(entry),
            Action::Preview => self.toggle_file_preview(&entry),
            Action::Reveal | Action::Copy | Action::CopyPath => {
                let Ok(url) = crate::macos::platform::files::file_url(&entry.path) else {
                    return;
                };
                if matches!(action, Action::Reveal) {
                    self.cancel_routing();
                    self.end_session();
                    NSWorkspace::sharedWorkspace()
                        .activateFileViewerSelectingURLs(&NSArray::from_slice(&[&*url]));
                } else {
                    let pasteboard = NSPasteboard::generalPasteboard();
                    pasteboard.clearContents();
                    if matches!(action, Action::CopyPath) {
                        unsafe {
                            pasteboard.setString_forType(
                                &NSString::from_str(&entry.path.to_string_lossy()),
                                NSPasteboardTypeString,
                            );
                        }
                    } else {
                        pasteboard
                            .writeObjects(&NSArray::from_slice(&[ProtocolObject::from_ref(&*url)]));
                    }
                    self.dismiss();
                }
            }
        }
    }
}
