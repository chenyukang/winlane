use super::*;

impl Delegate {
    pub(super) fn refresh_meeting(&self) {
        let state = self.ivars();
        if !self.searching_meeting() {
            return;
        }
        self.cancel_scoped_refresh();
        if state.meeting_receiver.borrow().is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        state.meeting_receiver.replace(Some(rx));
        let offset = state.meeting_day_offset.get();
        let wake = state.wake.get().unwrap().handle();
        std::thread::spawn(move || {
            let _ = tx.send(crate::macos::platform::meeting::load(offset));
            wake.signal();
        });
    }

    // Move to another day; > goes forward, < goes back.
    pub(super) fn shift_meeting_day(&self, delta: i32) {
        let offset = self.ivars().meeting_day_offset.get().saturating_add(delta);
        self.ivars().meeting_day_offset.set(offset);
        // Abandon the in-flight load for the old day and show a loading state.
        self.ivars().meeting_receiver.take();
        self.ivars().meeting_results.borrow_mut().items.clear();
        self.refresh_meeting();
        self.filter_preserving(None);
    }

    pub(super) fn meeting_day_key(&self, event: &NSEvent, composing: bool) -> Option<i32> {
        if composing || !self.searching_meeting() || event.r#type() != NSEventType::KeyDown {
            return None;
        }
        if event.modifierFlags().intersection(
            NSEventModifierFlags::Command
                | NSEventModifierFlags::Control
                | NSEventModifierFlags::Option,
        ) != NSEventModifierFlags::empty()
        {
            return None;
        }
        match event.characters()?.to_string().as_str() {
            ">" => Some(1),
            "<" => Some(-1),
            _ => None,
        }
    }

    pub(super) fn handle_meeting_key(&self, event: &NSEvent, composing: bool) -> bool {
        let Some(delta) = self.meeting_day_key(event, composing) else {
            return false;
        };
        self.shift_meeting_day(delta);
        true
    }

    pub(super) fn poll_meeting(&self) {
        let result = self
            .ivars()
            .meeting_receiver
            .borrow()
            .as_ref()
            .map(|rx| rx.try_recv());
        let meetings = match result {
            Some(Ok(meetings)) => meetings,
            Some(Err(TryRecvError::Disconnected)) => winlane::features::meeting::Meetings {
                items: Vec::new(),
                error: Some(
                    tr!(
                        "会议读取中断，按 ⌘R 重试。",
                        "Meeting loading stopped. Press ⌘R to retry."
                    )
                    .into(),
                ),
                access_denied: false,
            },
            _ => return,
        };
        let state = self.ivars();
        state.meeting_receiver.take();
        let selected = self.selected_result();
        {
            let mut cached = state.meeting_results.borrow_mut();
            // Keep cached rows on a failed read, but accept an empty successful load.
            if !meetings.items.is_empty() || meetings.error.is_none() {
                cached.items = meetings.items;
            }
            cached.error = meetings.error;
            cached.access_denied = meetings.access_denied;
        }
        if self.searching_meeting() {
            self.filter_preserving(selected);
        }
    }

    pub(super) fn clear_meeting_matches(&self) {
        self.ivars().meeting_matches.borrow_mut().clear();
        for ui in self.panels() {
            for row in ui.rows.borrow_mut().iter_mut() {
                if matches!(row.content, Some(RowContent::Meeting(_))) {
                    row.content = None;
                    row.app.setStringValue(&NSString::from_str(""));
                    row.title.setStringValue(&NSString::from_str(""));
                    row.button.setToolTip(None);
                    row.button.setAccessibilityLabel(None);
                }
            }
        }
    }

    pub(super) fn filter_meeting(&self, selected: Option<SelectedResult>) {
        let state = self.ivars();
        let items = winlane::features::meeting::matching(
            &state.meeting_results.borrow().items,
            &state.query.borrow(),
        );
        let selected = if let Some(SelectedResult::Meeting(id)) = selected {
            items.iter().position(|item| item.id == id)
        } else {
            None
        };
        self.clear_result_matches();
        state.meeting_matches.replace(items);
        state.selected.set(selected.unwrap_or(0));
        self.render();
    }

    pub(super) fn selected_meeting(&self) -> Option<winlane::features::meeting::Meeting> {
        if !self.searching_meeting() {
            return None;
        }
        self.ivars()
            .meeting_matches
            .borrow()
            .get(self.ivars().selected.get())
            .cloned()
    }

    pub(super) fn submit_meeting(&self) {
        let Some(meeting) = self.selected_meeting() else {
            return;
        };
        let Some(link) = meeting.link else {
            self.ivars().meeting_results.borrow_mut().error = Some(
                tr!(
                    "该会议没有可打开的链接。",
                    "This meeting has no link to open."
                )
                .into(),
            );
            self.render();
            return;
        };
        let Some(url) = NSURL::URLWithString(&NSString::from_str(&link)) else {
            self.ivars().meeting_results.borrow_mut().error =
                Some(trf!("无法打开链接：{}", "Could not open link: {}", link));
            self.render();
            return;
        };
        self.cancel_routing();
        self.end_session();
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}
