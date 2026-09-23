use super::*;

impl Delegate {
    pub(super) fn is_open_url_input_key(&self, event: &NSEvent, composing: bool) -> bool {
        self.searching_open_url()
            && !composing
            && event.r#type() == NSEventType::KeyDown
            && matches!(event.keyCode(), 36 | 76)
            && event.modifierFlags().intersection(
                NSEventModifierFlags::Command
                    | NSEventModifierFlags::Control
                    | NSEventModifierFlags::Option
                    | NSEventModifierFlags::Shift,
            ) == NSEventModifierFlags::Control
    }

    pub(super) fn refresh_open_url(&self) {
        let state = self.ivars();
        if !self.searching_open_url() {
            return;
        }
        self.cancel_scoped_refresh();
        if state.open_url_receiver.borrow().is_some() {
            return;
        }
        let Some(home) = std::env::var_os("HOME") else {
            state.open_url_history.borrow_mut().error =
                Some(tr!("找不到用户主目录。", "Home directory unavailable.").into());
            return;
        };
        let (tx, rx) = mpsc::channel();
        state.open_url_receiver.replace(Some(rx));
        let wake = state.wake.get().unwrap().handle();
        std::thread::spawn(move || {
            let root = winlane::features::open_url::chrome_directory(std::path::Path::new(&home));
            let _ = tx.send(winlane::features::open_url::load(&root));
            wake.signal();
        });
    }

    pub(super) fn poll_open_url(&self) {
        let result = self
            .ivars()
            .open_url_receiver
            .borrow()
            .as_ref()
            .map(|rx| rx.try_recv());
        let history = match result {
            Some(Ok(history)) => history,
            Some(Err(TryRecvError::Disconnected)) => winlane::features::open_url::History {
                pages: Vec::new(),
                error: Some(
                    tr!(
                        "历史记录读取中断，按 ⌘R 重试。",
                        "History loading stopped. Press ⌘R to retry."
                    )
                    .into(),
                ),
                access_denied: false,
            },
            _ => return,
        };
        let state = self.ivars();
        state.open_url_receiver.take();
        let selected = self.selected_result();
        {
            let mut cached = state.open_url_history.borrow_mut();
            // Keep usable cached rows on a failed read, but accept an empty
            // successful read so clearing Chrome history clears our cache too.
            if !history.pages.is_empty() || history.error.is_none() {
                cached.pages = history.pages;
            }
            cached.error = history.error;
            cached.access_denied = history.access_denied;
        }
        if self.searching_open_url() {
            self.filter_preserving(selected);
        }
    }

    pub(super) fn clear_open_url_matches(&self) {
        self.ivars().open_url_matches.borrow_mut().clear();
        for ui in self.panels() {
            for row in ui.rows.borrow_mut().iter_mut() {
                if matches!(row.content, Some(RowContent::OpenUrl(_))) {
                    row.content = None;
                    row.app.setStringValue(&NSString::from_str(""));
                    row.title.setStringValue(&NSString::from_str(""));
                    row.button.setToolTip(None);
                    row.button.setAccessibilityLabel(None);
                }
            }
        }
    }

    pub(super) fn filter_open_url(&self, selected: Option<SelectedResult>) {
        let state = self.ivars();
        let pages = winlane::features::open_url::matching(
            &state.open_url_history.borrow().pages,
            &state.query.borrow(),
        );
        let selected = if let Some(SelectedResult::OpenUrl(url)) = selected {
            pages.iter().position(|page| page.url == url)
        } else {
            None
        };
        self.clear_result_matches();
        state.open_url_matches.replace(pages);
        state.selected.set(selected.unwrap_or(0));
        self.render();
    }

    pub(super) fn selected_url(&self) -> Option<winlane::features::open_url::Page> {
        if !self.searching_open_url() {
            return None;
        }
        self.ivars()
            .open_url_matches
            .borrow()
            .get(self.ivars().selected.get())
            .cloned()
    }

    pub(super) fn open_url_target(
        &self,
        use_input: bool,
    ) -> Option<winlane::features::open_url::InputTarget> {
        if !self.searching_open_url() {
            return None;
        }
        if !use_input && let Some(page) = self.selected_url() {
            return Some(winlane::features::open_url::InputTarget::Url(page.url));
        }
        winlane::features::open_url::input_target(&self.ivars().query.borrow())
    }

    pub(super) fn submit_open_url(&self, use_input: bool) {
        let Some(target) = self.open_url_target(use_input) else {
            return;
        };
        match crate::macos::platform::open_url::PreparedPage::new(target.url()) {
            Ok(prepared) => {
                self.cancel_routing();
                self.end_session();
                self.ivars().launch_receiver.replace(Some(PendingLaunch {
                    receiver: prepared.open(self.ivars().wake.get().unwrap().handle()),
                    origin: LaunchOrigin::Search,
                }));
            }
            Err(error) => {
                self.ivars().open_url_history.borrow_mut().error = Some(error);
                self.render();
            }
        }
    }
}
