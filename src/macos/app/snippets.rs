use super::*;

impl Delegate {
    pub(super) fn filter_snippets(&self, selected_id: Option<SelectedResult>) {
        let state = self.ivars();
        let query = state.query.borrow();
        let config = state.config.borrow();
        let snippets = if query.trim().is_empty() {
            config.snippets.clone()
        } else {
            winlane::features::snippets::matching(&config.snippets, &query)
                .into_iter()
                .map(|index| config.snippets[index].clone())
                .collect()
        };
        let selected = if let Some(SelectedResult::Snippet(id)) = selected_id {
            snippets
                .iter()
                .position(|snippet| snippet.id == id)
                .unwrap_or(0)
        } else {
            0
        };
        drop(config);
        drop(query);
        state.matches.borrow_mut().clear();
        state.launch_matches.borrow_mut().clear();
        state.command_matches.borrow_mut().clear();
        state.application_icons.borrow_mut().clear();
        state.snippet_matches.replace(snippets);
        state.quicklink_matches.borrow_mut().clear();
        state.clipboard_matches.borrow_mut().clear();
        state.selected.set(selected);
        self.render();
    }

    pub(super) fn ensure_snippet_editor(
        &self,
    ) -> Retained<crate::macos::ui::snippets::SnippetEditor> {
        if let Some(editor) = self.ivars().snippet_editor.borrow().as_ref() {
            return editor.clone();
        }
        let weak = Weak::new(self);
        let editor = crate::macos::ui::snippets::SnippetEditor::new(
            self.ivars().config.borrow().snippets.clone(),
            Box::new(move |snippets| {
                let Some(delegate) = weak.load() else {
                    return Err("Winlane closed".into());
                };
                let mut config = delegate.ivars().config.borrow().clone();
                config.snippets = snippets;
                delegate.apply_config(config)
            }),
            self.mtm(),
        );
        self.ivars().snippet_editor.replace(Some(editor.clone()));
        editor
    }

    pub(super) fn selected_snippet(&self) -> Option<winlane::features::snippets::Snippet> {
        let index = self
            .ivars()
            .selected
            .get()
            .checked_sub(self.ivars().command_matches.borrow().len())?;
        self.ivars().snippet_matches.borrow().get(index).cloned()
    }

    pub(super) fn use_snippet(&self, snippet: &winlane::features::snippets::Snippet) {
        let Some(target) = NSRunningApplication::runningApplicationWithProcessIdentifier(
            self.ivars().previous_pid.get(),
        ) else {
            self.report_switch_error(tr!(
                "目标应用已退出，请重新打开搜索。",
                "The target app has quit. Reopen search."
            ));
            return;
        };
        let template = match winlane::features::snippets::Template::parse(&snippet.body) {
            Ok(template) => template,
            Err(error) => {
                self.report_switch_error(&error);
                return;
            }
        };
        let clipboard = if template.uses_clipboard() {
            crate::macos::platform::template_context::clipboard()
        } else {
            String::new()
        };
        if clipboard.len() > winlane::features::snippets::MAX_RENDERED_BYTES {
            self.report_switch_error(tr!(
                "剪贴板文字超过 1 MB，请先复制较短的内容。",
                "Clipboard text exceeds 1 MB. Copy a shorter selection first."
            ));
            return;
        }
        self.cancel_routing();
        self.end_session();
        if let Some(previous) = self.ivars().snippet_arguments.take() {
            previous.window().close();
        }
        if template.arguments.is_empty() {
            let result = crate::macos::platform::template_context::render(
                &template,
                &clipboard,
                &HashMap::new(),
            )
            .and_then(|text| self.paste_snippet(target, text));
            if let Err(error) = result {
                self.selection_failed(&error);
            }
        } else {
            let weak = Weak::new(self);
            let form = crate::macos::ui::snippets::SnippetArguments::new(
                &snippet.name,
                template,
                clipboard,
                Box::new(move |text| {
                    let Some(delegate) = weak.load() else {
                        return Err("Winlane closed".into());
                    };
                    delegate.paste_snippet(target.clone(), text)
                }),
                self.mtm(),
            );
            form.show();
            self.ivars().snippet_arguments.replace(Some(form));
        }
    }

    pub(super) fn paste_snippet(
        &self,
        target: Retained<NSRunningApplication>,
        text: String,
    ) -> Result<(), String> {
        if let Some(timer) = self.ivars().snippet_paste_timer.take() {
            timer.invalidate();
        }
        let weak = Weak::new(self);
        let timer = crate::macos::platform::paste::start(target, text, self.mtm(), move |error| {
            if let Some(delegate) = weak.load() {
                delegate.selection_failed(&error);
            }
        })?;
        self.ivars().snippet_paste_timer.replace(Some(timer));
        Ok(())
    }
}
