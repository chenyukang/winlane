use super::*;

impl Delegate {
    pub(super) fn open_quicklink_shortcut(&self, id: &str, session: u64) {
        let link = self
            .ivars()
            .config
            .borrow()
            .quicklinks
            .iter()
            .find(|link| link.id == id && link.shortcut.is_some())
            .cloned();
        let Some(link) = link else { return };
        let template = match winlane::features::quicklinks::Template::parse(&link.link) {
            Ok(template) => template,
            Err(error) => {
                self.report_quicklink_shortcut_error(session, &error);
                return;
            }
        };
        if template.arguments.is_empty() {
            if let Some(settings) = self.settings_window() {
                settings.window.orderOut(None);
            }
            if let Err(error) = self.open_quicklink_destination(&link, &template) {
                self.report_quicklink_shortcut_error(session, &error);
            }
        } else {
            self.prepare_quicklink_shortcut_input(link, template, session);
            self.present_panels();
            self.schedule_cache_warmup();
        }
    }

    fn report_quicklink_shortcut_error(&self, session: u64, error: &str) {
        if let Some(tap) = self.ivars().shortcut_tap.borrow().as_ref() {
            tap.resume_search(session);
        }
        self.show_mode(PanelMode::Search, session, 0);
        self.report_switch_error(error);
    }

    pub(super) fn prepare_quicklink_shortcut_input(
        &self,
        link: winlane::features::quicklinks::Quicklink,
        template: winlane::features::quicklinks::Template,
        session: u64,
    ) {
        // Prepare the normal search/input-source session before showing its argument fields.
        self.prepare_panel(PanelMode::Search, session, 0);
        if let Some(tap) = self.ivars().shortcut_tap.borrow().as_ref() {
            tap.resume_search(session);
        }
        self.begin_quicklink_input(link, template);
    }

    pub(super) fn ensure_quicklink_editor(
        &self,
    ) -> Retained<crate::macos::ui::quicklinks::QuicklinkEditor> {
        if let Some(editor) = self.ivars().quicklink_editor.borrow().as_ref() {
            return editor.clone();
        }
        let weak = Weak::new(self);
        let editor = crate::macos::ui::quicklinks::QuicklinkEditor::new(
            self.ivars().config.borrow().quicklinks.clone(),
            Box::new(move |links| {
                let Some(delegate) = weak.load() else {
                    return Err("Winlane closed".into());
                };
                let mut config = delegate.ivars().config.borrow().clone();
                config.quicklinks = links;
                delegate.apply_config(config)
            }),
            self.mtm(),
        );
        self.ivars().quicklink_editor.replace(Some(editor.clone()));
        editor
    }

    pub(super) fn filter_quicklinks(&self, selected: Option<SelectedResult>) {
        let state = self.ivars();
        let config = state.config.borrow();
        let links: Vec<_> =
            winlane::features::quicklinks::matching(&config.quicklinks, &state.query.borrow())
                .into_iter()
                .map(|i| config.quicklinks[i].clone())
                .collect();
        let selected = if let Some(SelectedResult::Quicklink(id)) = selected {
            links.iter().position(|q| q.id == id).unwrap_or(0)
        } else {
            0
        };
        drop(config);
        state.matches.borrow_mut().clear();
        state.launch_matches.borrow_mut().clear();
        state.command_matches.borrow_mut().clear();
        state.snippet_matches.borrow_mut().clear();
        state.clipboard_matches.borrow_mut().clear();
        state.application_icons.borrow_mut().clear();
        state.quicklink_matches.replace(links);
        state.selected.set(selected);
        self.render();
    }

    pub(super) fn selected_quicklink(&self) -> Option<winlane::features::quicklinks::Quicklink> {
        let state = self.ivars();
        let index = state.selected.get().checked_sub(
            state.command_matches.borrow().len()
                + state.snippet_matches.borrow().len()
                + state.clipboard_matches.borrow().len()
                + state.matches.borrow().len(),
        )?;
        state.quicklink_matches.borrow().get(index).cloned()
    }

    pub(super) fn editing_quicklink(&self) -> bool {
        self.ivars().quicklink_input.borrow().is_some()
    }

    pub(super) fn clear_quicklink_input(&self) {
        self.ivars().quicklink_input.take();
        for ui in self.panels() {
            ui.quicklink_bar.borrow_mut().clear();
            for row in ui.rows.borrow_mut().iter_mut() {
                if matches!(row.content, Some(RowContent::Quicklink(_))) {
                    row.content = None;
                }
            }
        }
    }

    pub(super) fn begin_selected_quicklink(&self) -> bool {
        if self.ivars().mode.get() != Some(PanelMode::Search) {
            return false;
        }
        let Some(link) = self.selected_quicklink() else {
            return false;
        };
        let Ok(template) = winlane::features::quicklinks::Template::parse(&link.link) else {
            return false;
        };
        if template.arguments.is_empty() {
            return false;
        }
        self.begin_quicklink_input(link, template);
        true
    }

    pub(super) fn begin_quicklink_input(
        &self,
        link: winlane::features::quicklinks::Quicklink,
        template: winlane::features::quicklinks::Template,
    ) {
        let clipboard = if template.uses_clipboard() {
            crate::macos::platform::template_context::clipboard()
        } else {
            String::new()
        };
        self.ivars().quicklink_input.replace(Some(
            crate::macos::ui::quicklinks::input::Input::new(link, template, clipboard),
        ));
        self.filter();
        self.focus_search();
    }

    pub(super) fn filter_quicklink_input(&self) {
        let state = self.ivars();
        let link = state
            .quicklink_input
            .borrow()
            .as_ref()
            .unwrap()
            .link
            .clone();
        state.matches.borrow_mut().clear();
        state.launch_matches.borrow_mut().clear();
        state.command_matches.borrow_mut().clear();
        state.snippet_matches.borrow_mut().clear();
        state.clipboard_matches.borrow_mut().clear();
        state.application_icons.borrow_mut().clear();
        state.quicklink_matches.replace(vec![link]);
        state.selected.set(0);
        self.render();
    }

    pub(super) fn remember_quicklink_field(&self, control: &NSControl) {
        for ui in self.panels() {
            let index = ui.quicklink_bar.borrow().index(control);
            if let Some(index) = index {
                if let Some(input) = self.ivars().quicklink_input.borrow_mut().as_mut() {
                    input.active = index;
                }
                break;
            }
        }
    }

    pub(super) fn update_quicklink_argument(&self, control: &NSControl) -> bool {
        if self.ivars().syncing_controls.get() || !self.editing_quicklink() {
            return false;
        }
        for ui in self.panels() {
            let changed = {
                let bar = ui.quicklink_bar.borrow();
                bar.index(control)
                    .map(|i| (i, bar.value(i).unwrap_or_default()))
            };
            if let Some((index, value)) = changed {
                if let Some(input) = self.ivars().quicklink_input.borrow_mut().as_mut() {
                    input.active = index;
                    input
                        .values
                        .insert(input.template.arguments[index].name.clone(), value);
                    input.error = None;
                }
                self.render();
                return true;
            }
        }
        false
    }

    pub(super) fn step_quicklink_argument(&self, direction: isize) {
        if let Some(input) = self.ivars().quicklink_input.borrow_mut().as_mut() {
            input.active = (input.active as isize + direction)
                .rem_euclid(input.template.arguments.len() as isize)
                as usize;
        }
        self.focus_search();
    }

    pub(super) fn quicklink_text_command(&self, control: &NSControl, command: Sel) -> bool {
        self.remember_quicklink_field(control);
        if command == sel!(insertTab:) {
            self.step_quicklink_argument(1);
        } else if command == sel!(insertBacktab:) {
            self.step_quicklink_argument(-1);
        } else if command == sel!(insertNewline:) {
            self.submit_quicklink_input();
        } else if command == sel!(cancelOperation:) {
            self.leave_scoped_search();
        } else if command == sel!(deleteBackward:) {
            let empty = self
                .ivars()
                .quicklink_input
                .borrow()
                .as_ref()
                .is_some_and(|input| {
                    input
                        .values
                        .get(&input.template.arguments[input.active].name)
                        .is_none_or(String::is_empty)
                });
            if !empty {
                return false;
            }
            let first = self
                .ivars()
                .quicklink_input
                .borrow()
                .as_ref()
                .is_some_and(|input| input.active == 0);
            if first {
                self.leave_scoped_search();
            } else {
                self.step_quicklink_argument(-1);
            }
        } else {
            return false;
        }
        true
    }

    pub(super) fn submit_quicklink_input(&self) {
        let result = {
            let input = self.ivars().quicklink_input.borrow();
            let Some(input) = input.as_ref() else {
                return;
            };
            input
                .destination()
                .map(|url| (url, input.link.open_with.clone()))
        };
        match result {
            Ok((url, application)) => {
                match crate::macos::platform::quicklinks::open(&url, &application) {
                    Ok(()) => {
                        self.cancel_routing();
                        self.end_session();
                    }
                    Err(error) => {
                        if let Some(input) = self.ivars().quicklink_input.borrow_mut().as_mut() {
                            input.error = Some(error);
                        }
                        self.render();
                    }
                }
            }
            Err(error) => {
                if let Some(input) = self.ivars().quicklink_input.borrow_mut().as_mut() {
                    input.error = Some(error);
                }
                self.render();
                self.focus_search();
            }
        }
    }

    pub(super) fn use_quicklink(&self, link: &winlane::features::quicklinks::Quicklink) {
        let template = match winlane::features::quicklinks::Template::parse(&link.link) {
            Ok(template) => template,
            Err(error) => {
                self.selection_failed(&error);
                return;
            }
        };
        if !template.arguments.is_empty() {
            self.begin_quicklink_input(link.clone(), template);
            return;
        }
        if let Err(error) = self.open_quicklink_destination(link, &template) {
            self.selection_failed(&error);
        }
    }

    fn open_quicklink_destination(
        &self,
        link: &winlane::features::quicklinks::Quicklink,
        template: &winlane::features::quicklinks::Template,
    ) -> Result<(), String> {
        let clipboard = if template.uses_clipboard() {
            crate::macos::platform::template_context::clipboard()
        } else {
            String::new()
        };
        let url =
            crate::macos::platform::quicklinks::render(template, &clipboard, &HashMap::new())?;
        self.cancel_routing();
        self.end_session();
        crate::macos::platform::quicklinks::open(&url, &link.open_with)
    }
}
