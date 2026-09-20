use super::*;
use crate::macos::platform::files::{Request, Service, Update};
use std::path::PathBuf;
use winlane::features::files::{self, Entry};

mod actions;
use actions::Action;
mod controls;
pub(super) use controls::Controls;

#[derive(Default)]
pub(super) struct State {
    service: Option<Service>,
    generation: u64,
    requested: Option<(String, files::Settings)>,
    pub menu_open: bool,
    pattern_pending: bool,
    entries: Vec<Entry>,
    pub recent: Vec<Entry>,
    recent_changed: bool,
    pub matches: Vec<Entry>,
    pub loading: bool,
    pub limited: bool,
    pub error: Option<String>,
    timer: Option<Retained<NSTimer>>,
    opening: Option<PendingOpen>,
}

struct PendingOpen {
    entry: Entry,
    session: u64,
    receiver: Receiver<Result<i32, String>>,
}

pub(super) fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

impl Delegate {
    pub(super) fn searching_files(&self) -> bool {
        self.scoped_search() && self.ivars().search_scope.get() == Some(SearchScope::Files)
    }

    pub(super) fn preload_files(&self) {
        let mut state = self.ivars().files.borrow_mut();
        if state.service.is_none() {
            state.service = Some(Service::new(
                home(),
                self.ivars().wake.get().unwrap().handle(),
            ));
        }
    }

    pub(super) fn filter_files(&self, selected: Option<SelectedResult>) {
        let query = self.ivars().query.borrow().clone();
        let settings = self.ivars().config.borrow().files.clone();
        let mut state = self.ivars().files.borrow_mut();
        let matcher = files::query::Matcher::literal(&query, &home());
        let changed = state.requested.as_ref() != Some(&(query.clone(), settings.clone()));
        if changed {
            if let Some(timer) = state.timer.take() {
                timer.invalidate();
            }
            if let Some(service) = &state.service {
                state.generation = service.cancel();
            }
            state.requested = Some((query.clone(), settings.clone()));
            state.loading = true;
            state.error = None;
            state.limited = false;
            state.pattern_pending = matcher.is_pattern();
            // Return to AppKit before any disk access, then debounce successive keystrokes.
            let timer = unsafe {
                NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                    if query.is_empty() { 0.0 } else { 0.10 },
                    self,
                    sel!(refreshFiles:),
                    None,
                    false,
                )
            };
            unsafe {
                NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes);
            }
            state.timer = Some(timer);
        }
        let home = home();
        let candidates: Vec<_> =
            if query.trim().is_empty() && state.loading && !state.recent.is_empty() {
                state
                    .recent
                    .iter()
                    .filter(|entry| settings.allows(&entry.path, &home))
                    .cloned()
                    .collect()
            } else {
                state
                    .entries
                    .iter()
                    .filter(|entry| {
                        if let Some(parent) = &matcher.directory {
                            if matcher.is_pattern() {
                                entry.path.starts_with(parent)
                            } else {
                                entry.path.parent() == Some(parent.as_path())
                            }
                        } else {
                            settings.allows(&entry.path, &home)
                        }
                    })
                    .cloned()
                    .collect()
            };
        state.matches = if matcher.is_pattern() && !state.pattern_pending {
            // The worker has already ranked literal matches ahead of pattern-only matches.
            candidates
        } else {
            matcher.matching(&candidates, &state.recent)
        };
        let position = match selected {
            Some(SelectedResult::File(path)) => {
                state.matches.iter().position(|entry| entry.path == path)
            }
            _ => None,
        };
        let empty = state.matches.is_empty();
        drop(state);
        self.ivars().selected.set(position.unwrap_or(0));
        self.ivars().matches.borrow_mut().clear();
        self.ivars().launch_matches.borrow_mut().clear();
        self.ivars().command_matches.borrow_mut().clear();
        self.ivars().snippet_matches.borrow_mut().clear();
        self.ivars().clipboard_matches.borrow_mut().clear();
        self.ivars().quicklink_matches.borrow_mut().clear();
        self.ivars().project_matches.borrow_mut().clear();
        self.ivars().open_url_matches.borrow_mut().clear();
        self.ivars().bluetooth_matches.borrow_mut().clear();
        self.ivars().keep_awake_matches.borrow_mut().clear();
        if changed || empty {
            self.close_file_preview();
        }
        self.render();
    }

    pub(super) fn files_timer_fired(&self, timer: &NSTimer) {
        if self
            .ivars()
            .files
            .borrow()
            .timer
            .as_ref()
            .is_some_and(|pending| std::ptr::eq(&**pending, timer))
        {
            self.refresh_files();
        }
    }

    pub(super) fn refresh_files(&self) {
        if !self.searching_files() {
            return;
        }
        self.preload_files();
        let mut state = self.ivars().files.borrow_mut();
        if let Some(timer) = state.timer.take() {
            timer.invalidate();
        }
        state.generation = state.service.as_ref().unwrap().cancel();
        state.loading = true;
        state.error = None;
        state.service.as_ref().unwrap().search(Request {
            generation: state.generation,
            query: self.ivars().query.borrow().clone(),
            settings: self.ivars().config.borrow().files.clone(),
            recent: state.recent.clone(),
        });
    }

    pub(super) fn cancel_files_search(&self) {
        let mut state = self.ivars().files.borrow_mut();
        if state.requested.take().is_none() {
            return;
        }
        if let Some(timer) = state.timer.take() {
            timer.invalidate();
        }
        if let Some(service) = &state.service {
            state.generation = service.cancel();
        }
        state.loading = false;
        state.matches.clear();
        drop(state);
        self.close_file_preview();
    }

    pub(super) fn poll_files(&self) {
        let updates: Vec<_> = self
            .ivars()
            .files
            .borrow()
            .service
            .as_ref()
            .map(|service| service.receiver.try_iter().collect())
            .unwrap_or_default();
        for update in updates {
            self.apply_files_update(update);
        }
        let result = self
            .ivars()
            .files
            .borrow()
            .opening
            .as_ref()
            .map(|pending| pending.receiver.try_recv());
        if let Some(Ok(result)) = result {
            let mut state = self.ivars().files.borrow_mut();
            let PendingOpen { entry, session, .. } = state.opening.take().unwrap();
            match result {
                Ok(_) => {
                    files::remember(&mut state.recent, entry);
                    state.recent_changed = true;
                    if let Some(service) = &state.service {
                        service.save(state.recent.clone());
                    }
                }
                Err(error) => {
                    state.error = Some(error.clone());
                    drop(state);
                    if self.ivars().session.get() == session && self.ivars().mode.get().is_none() {
                        self.selection_failed(&error);
                    }
                }
            }
        }
    }

    pub(super) fn apply_files_update(&self, update: Update) {
        let selected = self.selected_result();
        let mut state = self.ivars().files.borrow_mut();
        match update {
            Update::Recent(recent) => {
                if !state.recent_changed {
                    state.recent = recent;
                }
                drop(state);
                if self.searching_files() && self.ivars().query.borrow().trim().is_empty() {
                    self.refresh_files();
                    self.filter_files(selected);
                }
                return;
            }
            Update::Results {
                generation,
                entries,
                gathering,
                limited,
                error,
            } => {
                if state.generation != generation || !self.searching_files() {
                    return;
                }
                if !entries.is_empty() || (!gathering && error.is_none()) {
                    state.entries = entries;
                    state.pattern_pending = false;
                }
                state.loading = gathering;
                state.limited = limited;
                state.error = error;
            }
        }
        drop(state);
        if self.searching_files() {
            self.filter_files(selected);
        }
    }

    pub(super) fn selected_file(&self) -> Option<Entry> {
        if !self.searching_files() {
            return None;
        }
        self.ivars()
            .files
            .borrow()
            .matches
            .get(self.ivars().selected.get())
            .cloned()
    }

    pub(super) fn open_selected_file(&self, open_folder: bool) {
        let Some(entry) = self.selected_file() else {
            return;
        };
        if entry.directory && !open_folder {
            self.complete_selected_file();
            return;
        }
        self.open_file_entry(entry);
    }

    fn open_file_entry(&self, entry: Entry) {
        self.preload_files();
        self.cancel_routing();
        self.end_session();
        let receiver = crate::macos::platform::files::open(
            &entry.path,
            self.ivars().wake.get().unwrap().handle(),
        );
        self.ivars().files.borrow_mut().opening = Some(PendingOpen {
            entry,
            session: self.ivars().session.get(),
            receiver,
        });
    }

    pub(super) fn clear_recent_files(&self) {
        self.preload_files();
        let mut state = self.ivars().files.borrow_mut();
        state.recent.clear();
        state.entries.clear();
        state.recent_changed = true;
        state.service.as_ref().unwrap().save(Vec::new());
        drop(state);
        if self.searching_files() {
            self.refresh_files();
            self.filter();
        }
    }

    pub(super) fn handle_files_key(&self, event: &NSEvent, composing: bool) -> bool {
        if !self.searching_files() || composing {
            return false;
        }
        let flags = event.modifierFlags().intersection(
            NSEventModifierFlags::Command
                | NSEventModifierFlags::Control
                | NSEventModifierFlags::Option
                | NSEventModifierFlags::Shift,
        );
        if self.is_files_open_key(event, composing) {
            if !event.isARepeat() {
                self.open_selected_file(true);
            }
            return true;
        }
        if flags.is_empty() && matches!(event.keyCode(), 36 | 76) && event.isARepeat() {
            return true;
        }
        if event.keyCode() == 51 && flags == NSEventModifierFlags::Control {
            let parent = files::query::parent_search_path(&self.ivars().query.borrow(), &home());
            if let Some(parent) = parent {
                self.set_file_query(parent);
                return true;
            }
        }
        if event.keyCode() == 48 && flags.is_empty() {
            if !event.isARepeat() {
                self.complete_selected_file();
            }
            return true;
        }
        let command = flags == NSEventModifierFlags::Command;
        if flags == NSEventModifierFlags::Control && event.keyCode() == 17 {
            if !event.isARepeat() {
                self.show_file_actions();
            }
            return true;
        }
        let path_copy = flags == (NSEventModifierFlags::Command | NSEventModifierFlags::Shift)
            && event.keyCode() == 8;
        if !(command && matches!(event.keyCode(), 36 | 76 | 16 | 8) || path_copy) {
            return false;
        }
        let Some(entry) = self.selected_file() else {
            return true;
        };
        let action = if event.keyCode() == 16 {
            Action::Preview
        } else if matches!(event.keyCode(), 36 | 76) {
            Action::Reveal
        } else if path_copy {
            Action::CopyPath
        } else {
            Action::Copy
        };
        self.perform_file_action(action, entry);
        true
    }

    pub(super) fn is_files_open_key(&self, event: &NSEvent, composing: bool) -> bool {
        self.searching_files()
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

    pub(super) fn complete_selected_file(&self) -> bool {
        if !self.searching_files() {
            return false;
        }
        let Some(entry) = self.selected_file() else {
            return true;
        };
        self.complete_file_entry(&entry);
        true
    }

    fn complete_file_entry(&self, entry: &Entry) {
        self.set_file_query(entry.completion(&home()));
    }

    fn set_file_query(&self, text: String) {
        let caret = objc2_foundation::NSRange::new(text.encode_utf16().count(), 0);
        self.ivars().query.replace(text);
        self.filter();
        // Keep the current editor and input source; only move its insertion point.
        for ui in self.panels() {
            if let Some(editor) = ui.input.currentEditor() {
                editor.setSelectedRange(caret);
            }
        }
    }

    pub(super) fn close_file_preview(&self) {
        for ui in self.panels() {
            if let Some(preview) = ui.file_preview.take() {
                unsafe {
                    let _: () = msg_send![&preview, setPreviewItem: std::ptr::null::<AnyObject>()];
                }
                preview.removeFromSuperview();
            }
            ui.scroll.setHidden(false);
        }
    }

    pub(super) fn toggle_file_preview(&self, entry: &Entry) {
        if self
            .panels()
            .iter()
            .any(|ui| ui.file_preview.borrow().is_some())
        {
            self.close_file_preview();
            return;
        }
        let Ok(url) = crate::macos::platform::files::file_url(&entry.path) else {
            return;
        };
        for ui in self.panels() {
            // SAFETY: QLPreviewView is an NSView; NSURL implements QLPreviewItem.
            // Embed it in the existing panel so preview never steals application focus.
            unsafe {
                let allocated: objc2::rc::Allocated<NSView> =
                    msg_send![objc2::class!(QLPreviewView), alloc];
                let preview: Retained<NSView> =
                    msg_send![allocated, initWithFrame: ui.scroll.frame(), style: 0usize];
                let _: () = msg_send![&preview, setAutostarts: false];
                let _: () = msg_send![&preview, setPreviewItem: &*url];
                ui.panel.contentView().unwrap().addSubview(&preview);
                ui.scroll.setHidden(true);
                ui.file_preview.replace(Some(preview));
            }
        }
    }
}

#[link(name = "Quartz", kind = "framework")]
unsafe extern "C" {}

#[cfg(test)]
#[path = "../../../tests/native/app/files.rs"]
pub(crate) mod tests;
