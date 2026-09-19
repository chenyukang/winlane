use super::*;

impl Delegate {
    pub(super) fn focus_open_url_input_at(&self, panel: &SearchPanel, point: NSPoint) {
        if !self.searching_open_url() || self.ivars().query.borrow().trim().is_empty() {
            return;
        }
        if self.panels().iter().any(|ui| {
            std::ptr::eq(&*ui.panel, panel)
                && objc2_foundation::NSPointInRect(
                    ui.input.convertPoint_fromView(point, None),
                    ui.input.bounds(),
                )
        }) {
            self.ivars().open_url_input_active.set(true);
            self.render();
        }
    }

    pub(super) fn refresh_open_url(&self) {
        let state = self.ivars();
        if !self.searching_open_url() || state.open_url_receiver.borrow().is_some() {
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
        self.ivars().open_url_receiver.take();
        if self.searching_open_url() {
            let selected = self.selected_result();
            self.ivars().open_url_history.replace(history);
            self.filter_preserving(selected);
        }
    }

    pub(super) fn clear_open_url(&self) {
        let state = self.ivars();
        state.open_url_input_active.set(false);
        if state.open_url_receiver.borrow().is_none()
            && state.open_url_matches.borrow().is_empty()
            && state.open_url_history.borrow().pages.is_empty()
            && state.open_url_history.borrow().error.is_none()
        {
            return;
        }
        self.ivars().open_url_receiver.take();
        self.ivars().open_url_matches.borrow_mut().clear();
        self.ivars().open_url_history.replace(Default::default());
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
        state
            .open_url_input_active
            .set(selected.is_none() && !state.query.borrow().trim().is_empty());
        state.matches.borrow_mut().clear();
        state.launch_matches.borrow_mut().clear();
        state.command_matches.borrow_mut().clear();
        state.snippet_matches.borrow_mut().clear();
        state.clipboard_matches.borrow_mut().clear();
        state.quicklink_matches.borrow_mut().clear();
        state.project_matches.borrow_mut().clear();
        state.application_icons.borrow_mut().clear();
        state.open_url_matches.replace(pages);
        state.selected.set(selected.unwrap_or(0));
        self.render();
    }

    pub(super) fn selected_url(&self) -> Option<winlane::features::open_url::Page> {
        if !self.searching_open_url() || self.ivars().open_url_input_active.get() {
            return None;
        }
        self.ivars()
            .open_url_matches
            .borrow()
            .get(self.ivars().selected.get())
            .cloned()
    }

    pub(super) fn open_url_target(&self) -> Option<winlane::features::open_url::InputTarget> {
        if !self.searching_open_url() {
            return None;
        }
        self.selected_url()
            .map(|page| winlane::features::open_url::InputTarget::Url(page.url))
            .or_else(|| winlane::features::open_url::input_target(&self.ivars().query.borrow()))
    }

    pub(super) fn open_selected_url(&self) {
        let Some(target) = self.open_url_target() else {
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
