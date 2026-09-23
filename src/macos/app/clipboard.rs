use super::*;

impl Delegate {
    pub(super) fn configure_clipboard_timer(&self) {
        if self.ivars().clipboard.borrow().is_none() {
            return;
        }
        if !self.ivars().config.borrow().clipboard.enabled {
            if let Some(timer) = self.ivars().clipboard_timer.take() {
                timer.invalidate();
            }
        } else if self.ivars().clipboard_timer.borrow().is_none() {
            // The main run loop retains the delegate; the selector accepts a timer argument.
            let timer = unsafe {
                NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                    0.5,
                    self,
                    sel!(pollClipboard:),
                    None,
                    true,
                )
            };
            timer.setTolerance(0.1);
            unsafe {
                NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes);
            }
            self.ivars().clipboard_timer.replace(Some(timer));
        }
    }

    pub(super) fn filter_clipboard(&self, selected_id: Option<SelectedResult>) {
        let state = self.ivars();
        let matches = state
            .clipboard
            .borrow()
            .as_ref()
            .map(|clipboard| clipboard.history.matching(&state.query.borrow()))
            .unwrap_or_default();
        let selected = if let Some(SelectedResult::Clipboard(id)) = selected_id {
            matches
                .iter()
                .position(|candidate| *candidate == id)
                .unwrap_or_else(|| state.selected.get().min(matches.len().saturating_sub(1)))
        } else {
            0
        };
        self.clear_result_matches();
        state.clipboard_matches.replace(matches);
        state.selected.set(selected);
        self.render();
    }

    pub(super) fn selected_clipboard(&self) -> Option<winlane::features::clipboard::Entry> {
        if !self.searching_clipboard() {
            return None;
        }
        let id = *self
            .ivars()
            .clipboard_matches
            .borrow()
            .get(self.ivars().selected.get())?;
        self.ivars()
            .clipboard
            .borrow()
            .as_ref()?
            .history
            .get(id)
            .cloned()
    }

    pub(super) fn use_clipboard(&self, paste: bool) {
        let Some(entry) = self.selected_clipboard() else {
            return;
        };
        let content = match crate::macos::platform::clipboard::PasteContent::from_entry(&entry) {
            Ok(content) => content,
            Err(error) => {
                self.report_switch_error(&error);
                return;
            }
        };
        if !paste {
            match content.write(&NSPasteboard::generalPasteboard()) {
                Ok(()) => self.dismiss(),
                Err(error) => self.report_switch_error(&error),
            }
            return;
        }
        let Some(target) = NSRunningApplication::runningApplicationWithProcessIdentifier(
            self.ivars().previous_pid.get(),
        ) else {
            self.report_switch_error(tr!(
                "目标应用已退出，请重新打开搜索。",
                "The target app has quit. Reopen search."
            ));
            return;
        };
        self.cancel_routing();
        self.end_session();
        if let Some(timer) = self.ivars().snippet_paste_timer.take() {
            timer.invalidate();
        }
        let weak = Weak::new(self);
        match crate::macos::platform::paste::start_content(
            target,
            content,
            self.mtm(),
            move |error| {
                if let Some(delegate) = weak.load() {
                    delegate.selection_failed(&error);
                }
            },
        ) {
            Ok(timer) => {
                self.ivars().snippet_paste_timer.replace(Some(timer));
            }
            Err(error) => self.selection_failed(&error),
        }
    }

    pub(super) fn delete_clipboard_entry(&self) {
        let Some(entry) = self.selected_clipboard() else {
            return;
        };
        if let Some(clipboard) = self.ivars().clipboard.borrow_mut().as_mut() {
            clipboard.remove(entry.id);
        }
        self.filter_preserving(Some(SelectedResult::Clipboard(entry.id)));
    }

    pub(super) fn toggle_clipboard_recording(&self) {
        let mut candidate = self.ivars().config.borrow().clone();
        candidate.clipboard.enabled = !candidate.clipboard.enabled;
        if let Err(error) = self.apply_config(candidate) {
            self.report_switch_error(&error);
        }
    }

    pub(super) fn clear_clipboard_history(&self) {
        if let Some(clipboard) = self.ivars().clipboard.borrow_mut().as_mut() {
            clipboard.clear();
        }
        if self.searching_clipboard() {
            self.filter();
        }
    }

    pub(super) fn confirm_clear_clipboard(&self) {
        let alert = NSAlert::new(self.mtm());
        alert.setMessageText(&NSString::from_str(tr!(
            "清空剪贴板历史？",
            "Clear clipboard history?"
        )));
        alert.setInformativeText(&NSString::from_str(tr!(
            "删除所有已记录的历史。当前系统剪贴板中的内容不会改变。",
            "Delete all recorded entries. The current system clipboard will stay unchanged."
        )));
        alert.addButtonWithTitle(&NSString::from_str(tr!("清空历史", "Clear History")));
        alert.addButtonWithTitle(&NSString::from_str(tr!("取消", "Cancel")));
        if alert.runModal() == NSAlertFirstButtonReturn {
            self.clear_clipboard_history();
        }
    }
}
