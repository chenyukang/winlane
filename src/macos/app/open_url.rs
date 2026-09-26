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
        self.reset_open_url_deep();
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
        self.poll_open_url_deep();
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

    /// Apply a finished full-history fallback search for the current query.
    fn poll_open_url_deep(&self) {
        let state = self.ivars();
        let result = state
            .open_url_deep_receiver
            .borrow()
            .as_ref()
            .map(|rx| rx.try_recv());
        match result {
            Some(Ok(result)) => {
                state.open_url_deep_receiver.take();
                // Reject a result whose query is no longer the active one.
                if *state.open_url_deep_query.borrow() != *state.query.borrow() {
                    return;
                }
                match result {
                    Ok(pages) => {
                        state.open_url_deep_pages.replace(pages);
                    }
                    Err(_) => state.open_url_deep_pages.borrow_mut().clear(),
                }
                state.open_url_deep_loaded.set(true);
                if self.searching_open_url() {
                    self.filter_preserving(self.selected_result());
                }
            }
            Some(Err(TryRecvError::Disconnected)) => {
                state.open_url_deep_receiver.take();
            }
            _ => {}
        }
    }

    /// Drop any cached or in-flight full-history fallback search.
    fn reset_open_url_deep(&self) {
        let state = self.ivars();
        state.open_url_deep_receiver.borrow_mut().take();
        state.open_url_deep_query.borrow_mut().clear();
        state.open_url_deep_pages.borrow_mut().clear();
        state.open_url_deep_loaded.set(false);
    }

    /// Search the whole `Default` profile in the background for queries the
    /// cached recent URLs cannot answer.
    fn ensure_open_url_deep_search(&self, query: &str) {
        let state = self.ivars();
        if state.open_url_deep_receiver.borrow().is_some()
            && *state.open_url_deep_query.borrow() == query
        {
            return;
        }
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        state.open_url_deep_receiver.replace(Some(rx));
        state.open_url_deep_query.replace(query.to_owned());
        state.open_url_deep_pages.borrow_mut().clear();
        state.open_url_deep_loaded.set(false);
        let wake = state.wake.get().unwrap().handle();
        let query = query.to_owned();
        std::thread::spawn(move || {
            let root = winlane::features::open_url::chrome_directory(std::path::Path::new(&home));
            let _ = tx.send(winlane::features::open_url::deep_search_default(
                &root, &query,
            ));
            wake.signal();
        });
    }

    pub(super) fn clear_open_url_matches(&self) {
        self.reset_open_url_deep();
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
        let query = state.query.borrow().clone();
        let mut pages =
            winlane::features::open_url::matching(&state.open_url_history.borrow().pages, &query);
        // When the cached recent URLs have no match, fall back to the whole
        // Default profile history (loaded once per query in the background).
        // Only when the recent window was actually full: a smaller list already
        // holds every URL, so there is nothing older to search.
        if pages.is_empty()
            && !query.trim().is_empty()
            && state.open_url_history.borrow().pages.len() >= winlane::features::open_url::MAX_URLS
        {
            let reused = {
                let deep_query = state.open_url_deep_query.borrow();
                *deep_query == query && state.open_url_deep_loaded.get()
            };
            if reused {
                pages = state.open_url_deep_pages.borrow().clone();
            } else {
                self.ensure_open_url_deep_search(&query);
            }
        }
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
