use super::*;
use std::borrow::Cow;

impl Delegate {
    pub(super) fn render(&self) {
        if self.ivars().mode.get().is_none() || self.ivars().preparing_panel.get() {
            return;
        }
        self.ivars().syncing_controls.set(true);
        for ui in self.panels() {
            self.render_panel(&ui);
        }
        self.ivars().syncing_controls.set(false);
    }

    pub(super) fn render_panel(&self, ui: &PanelUi) {
        #[cfg(test)]
        self.ivars()
            .render_passes
            .set(self.ivars().render_passes.get() + 1);
        let opacity = f64::from(self.ivars().config.borrow().background_opacity) / 100.0;
        ui.backdrop.set_opacity(opacity);
        let query = self.ivars().query.borrow();
        // The field's value can include uncommitted pinyin while the shared
        // query still contains the last committed text. A background refresh
        // must not replace that live composition with the stale query.
        let composing = ui
            .input
            .currentEditor()
            .and_then(|editor| editor.downcast::<NSTextView>().ok())
            .is_some_and(|editor| NSTextInputClient::hasMarkedText(&*editor));
        if !composing && ui.input.stringValue().to_string() != *query {
            ui.input.setStringValue(&NSString::from_str(&query));
        }
        drop(query);
        let state = self.ivars();
        let list = &ui.list;
        let windows = state.windows.borrow();
        let matched = state.matches.borrow();
        let launch_matches = state.launch_matches.borrow();
        let command_matches = state.command_matches.borrow();
        let apps = state.installed_apps.borrow();
        let snippet_matches = state.snippet_matches.borrow();
        let quicklink_matches = state.quicklink_matches.borrow();
        let project_matches = state.project_matches.borrow();
        let project_cache = state.project_cache.borrow();
        let in_projects = self.searching_projects();
        let clipboard_matches = state.clipboard_matches.borrow();
        let clipboard = state.clipboard.borrow();
        let extra_count = command_matches.len() + snippet_matches.len() + clipboard_matches.len();
        let count = extra_count
            + matched.len()
            + quicklink_matches.len()
            + launch_matches.len()
            + project_matches.len();
        let quicklink_input = state.quicklink_input.borrow();
        let inline = quicklink_input.is_some();
        let trusted = accessibility::is_trusted();
        let demo = state.demo.get();
        let switching = state.mode.get() == Some(PanelMode::Switch);
        let snippets = self.searching_snippets();
        let in_clipboard = self.searching_clipboard();
        let show_hints = state.config.borrow().show_usage_hints;
        let density = state.config.borrow().display_density;
        let row_height = row_height(density);
        let show_mode_label =
            switching && (show_hints || !state.alias_input.borrow().text().is_empty());
        let root = ui.panel.contentView().unwrap();
        let previous_height = root.bounds().size.height;
        let mut frame = ui.panel.frame();
        let chrome_height = frame.size.height - previous_height;
        let visible = ui.panel.screen().map(|screen| screen.visibleFrame());
        let argument_height = quicklink_input
            .as_ref()
            .map_or(0.0, |input| input.extra_height(density));
        let mut height = panel_height(count, switching, !trusted || demo, show_mode_label, density)
            + argument_height;
        if let Some(visible) = visible {
            height = height.min((visible.size.height - chrome_height).max(1.0));
        }
        if (height - previous_height).abs() > 0.5 {
            // Keep the search field stationary as results shrink; only shift the
            // top edge when growing would otherwise extend below the display.
            frame.origin.y += previous_height - height;
            frame.size.height = height + chrome_height;
            if let Some(visible) = visible {
                frame.origin.y = frame.origin.y.clamp(
                    visible.origin.y,
                    (visible.origin.y + visible.size.height - frame.size.height)
                        .max(visible.origin.y),
                );
            }
            ui.panel.setFrame_display(frame, false);
        }
        let (input_height, input_font_size, input_control_size) = match density {
            DisplayDensity::Compact => (34.0, 15.0, NSControlSize::Regular),
            DisplayDensity::Normal => (38.0, 16.0, NSControlSize::Large),
        };
        if ui.input.controlSize() != input_control_size {
            ui.input.setControlSize(input_control_size);
        }
        if ui.input.frame().size.height != input_height {
            ui.input
                .setFrameSize(NSSize::new(ui.input.frame().size.width, input_height));
        }
        if ui
            .input
            .font()
            .is_none_or(|font| font.pointSize() != input_font_size)
        {
            ui.input
                .setFont(Some(&NSFont::systemFontOfSize(input_font_size)));
        }
        let header: [(&NSView, f64); 4] = [
            (&ui.clipboard_actions, height - 50.0),
            (&ui.input, height - 35.0 - input_height / 2.0),
            (&ui.scope_back, height - 52.0),
            (
                &ui.shortcut_label,
                height - if switching { 26.0 } else { 44.0 },
            ),
        ];
        for (view, y) in header {
            let origin = NSPoint::new(view.frame().origin.x, y);
            if view.frame().origin != origin {
                view.setFrameOrigin(origin);
            }
        }
        let alias_input = state.alias_input.borrow();
        let alias_query = alias_input.text();
        let aliases = state.aliases.borrow();
        let alias_match = self.alias_match();
        let unmatched_alias =
            switching && !alias_query.is_empty() && alias_match.position().is_none();
        if let Some(input) = quicklink_input.as_ref() {
            ui.quicklink_bar.borrow_mut().render(
                input,
                rect(
                    0.0,
                    height - 64.0 - argument_height,
                    WIDTH,
                    64.0 + argument_height,
                ),
                density,
                ProtocolObject::from_ref(self),
                self.mtm(),
            );
        } else {
            ui.quicklink_bar.borrow().view.setHidden(true);
        }
        ui.input.setHidden(switching || inline);
        ui.scope_back.setHidden(!self.scoped_search() || inline);
        ui.scope_back.setTitle(&NSString::from_str(if in_clipboard {
            tr!("‹ 剪贴板", "‹ Clipboard")
        } else if in_projects {
            tr!("‹ 项目", "‹ Projects")
        } else if self.searching_quicklinks() {
            tr!("‹ 链接", "‹ Links")
        } else {
            tr!("‹ 片段", "‹ Snippets")
        }));
        ui.clipboard_actions.setHidden(!in_clipboard);
        ui.shortcut_label.setHidden(in_clipboard || inline);
        if let Some(item) = ui.clipboard_actions.itemAtIndex(3) {
            item.setTitle(&NSString::from_str(
                if state.config.borrow().clipboard.enabled {
                    tr!("暂停记录", "Pause recording")
                } else {
                    tr!("恢复记录", "Resume recording")
                },
            ));
        }
        ui.mode_label.setHidden(!show_mode_label);
        ui.settings_button.setHidden(!show_hints);
        let config = state.config.borrow();
        ui.mode_label
            .setStringValue(&NSString::from_str(&if alias_query.is_empty() {
                trf!(
                    "切换模式 · {} 选择 · 松开 {} 或 ↵ 确认",
                    "Switch · {} to select · Release {} or ↵ to confirm",
                    config.switch_shortcut.display(),
                    config.switch_shortcut.release_label()
                )
            } else {
                format!(
                    "Alias  {alias_query}  ·  {}",
                    if unmatched_alias {
                        if alias_match == AliasMatch::Ambiguous {
                            tr!(
                                "多个应用匹配 · 继续输入第二个字母",
                                "Multiple matches · Type a second letter"
                            )
                        } else {
                            tr!(
                                "没有匹配窗口 · Backspace 修改",
                                "No matching window · Backspace to edit"
                            )
                        }
                    } else {
                        tr!(
                            "松开修饰键确认 · Backspace 修改",
                            "Release modifier to confirm · Backspace to edit"
                        )
                    }
                )
            }));
        ui.shortcut_label
            .setStringValue(&NSString::from_str(&if switching {
                config.switch_shortcut.display()
            } else {
                config.shortcut.display()
            }));
        ui.shortcut_label
            .setToolTip(Some(&NSString::from_str(&if switching {
                config.switch_shortcut.display()
            } else {
                config.search_shortcuts_display()
            })));
        drop(config);
        ui.help.setHidden(trusted || demo);
        ui.refresh_button.setHidden(trusted || demo);
        ui.demo_button.setHidden(trusted && !demo);
        ui.demo_button.setTitle(&NSString::from_str(if demo {
            tr!("返回真实窗口", "Show Real Windows")
        } else {
            tr!("查看演示", "View Demo")
        }));
        let list_bottom = if !trusted || demo { 64.0 } else { LIST_BOTTOM };
        let mode_frame = rect(16.0, list_bottom, WIDTH - 32.0, 20.0);
        if ui.mode_label.frame() != mode_frame {
            ui.mode_label.setFrame(mode_frame);
        }
        let list_bottom = list_bottom
            + if show_mode_label {
                MODE_LABEL_SPACING
            } else {
                0.0
            };
        let list_top = height - if switching { 36.0 } else { HEIGHT - LIST_TOP } - argument_height;
        let list_height = list_top - list_bottom;
        let scroll_frame = rect(10.0, list_bottom, LIST_WIDTH, list_height);
        if ui.scroll.frame() != scroll_frame {
            ui.scroll.setFrame(scroll_frame);
        }
        let list_size = NSSize::new(LIST_WIDTH, (count as f64 * row_height).max(list_height));
        if list.frame().size != list_size {
            list.setFrameSize(list_size);
        }
        let mut rows = ui.rows.borrow_mut();
        if rows
            .first()
            .is_some_and(|row| row.button.ivars().density.get() != density)
        {
            for row in rows.drain(..) {
                row.button.removeFromSuperview();
            }
        }
        for row in rows.iter_mut().skip(count) {
            if row.attached {
                row.button.removeFromSuperview();
                row.attached = false;
            }
        }
        if !in_clipboard
            && rows
                .iter()
                .any(|row| matches!(row.content, Some(RowContent::Clipboard(..))))
        {
            for row in rows.drain(..) {
                row.button.removeFromSuperview();
            }
        }
        rows.truncate(if in_clipboard {
            count
        } else {
            count.max(windows.len().min(128))
        });
        for label in ui.empty_labels.borrow_mut().drain(..) {
            label.removeFromSuperview();
        }
        if count == 0 {
            let (title, detail) = if in_clipboard {
                if clipboard
                    .as_ref()
                    .is_some_and(|clipboard| clipboard.loading)
                {
                    (
                        tr!("正在读取历史…", "Loading history…"),
                        tr!("可以继续输入搜索。", "You can keep typing."),
                    )
                } else if state.query.borrow().trim().is_empty() {
                    (
                        tr!("还没有剪贴板历史", "No clipboard history yet"),
                        tr!(
                            "复制文本或图片后，它会出现在这里。可在设置 → 剪贴板中管理记录。",
                            "Copy text or an image to see it here. Manage recording in Settings → Clipboard."
                        ),
                    )
                } else {
                    (
                        tr!("没有匹配的剪贴板记录", "No matching clipboard entries"),
                        tr!(
                            "试试其他关键词，或按 Esc 返回。",
                            "Try other keywords, or press Esc to go back."
                        ),
                    )
                }
            } else if in_projects {
                if state.project_receiver.borrow().is_some() {
                    (
                        tr!("正在读取项目…", "Loading projects…"),
                        tr!("可以继续输入搜索。", "You can keep typing."),
                    )
                } else if project_cache.error.is_some() {
                    (
                        tr!("无法读取最近项目", "Could not load recent projects"),
                        tr!(
                            "按 ⌘R 重试，或按 Esc 返回。",
                            "Press ⌘R to retry, or Esc to go back."
                        ),
                    )
                } else if project_cache.projects.is_empty() {
                    (
                        tr!("还没有本地最近项目", "No recent local projects"),
                        tr!(
                            "先在 VS Code 打开文件夹或工作区，再按 ⌘R 刷新。",
                            "Open a folder or workspace in VS Code, then press ⌘R to refresh."
                        ),
                    )
                } else {
                    (
                        tr!("没有匹配的项目", "No matching projects"),
                        tr!(
                            "按项目名或路径搜索，或按 Esc 返回。",
                            "Search by project name or path, or press Esc to go back."
                        ),
                    )
                }
            } else if self.searching_quicklinks() {
                (
                    tr!("没有匹配的快捷链接", "No matching quicklinks"),
                    tr!(
                        "在设置 → 快捷链接中添加或导入链接，或按 Esc 返回。",
                        "Add or import links in Settings → Quicklinks, or press Esc to go back."
                    ),
                )
            } else if snippets {
                if state.config.borrow().snippets.is_empty() {
                    (
                        tr!("还没有片段", "No snippets yet"),
                        tr!(
                            "在设置 → 片段中新建，或按 Esc 返回窗口搜索。",
                            "Create one in Settings → Snippets, or press Esc to return to window search."
                        ),
                    )
                } else {
                    (
                        tr!("没有匹配的片段", "No matching snippets"),
                        tr!(
                            "按名称或正文搜索，或按 Esc 返回窗口搜索。",
                            "Search by name or content, or press Esc to return to window search."
                        ),
                    )
                }
            } else if state.loading.get() {
                (
                    tr!("正在读取窗口…", "Reading windows…"),
                    tr!(
                        "窗口枚举在后台进行，搜索界面仍可输入。",
                        "Loading windows in the background. You can keep typing."
                    ),
                )
            } else if !switching
                && !state.query.borrow().trim().is_empty()
                && state.catalog_receiver.borrow().is_some()
            {
                (
                    tr!("正在查找应用…", "Finding apps…"),
                    tr!("可以继续输入关键词。", "You can keep typing."),
                )
            } else if !trusted && !demo {
                (
                    tr!(
                        "先允许 Winlane 控制窗口",
                        "Allow Winlane to control windows"
                    ),
                    tr!(
                        "在系统设置 → 隐私与安全性的应用控制权限中启用 Winlane。\nmacOS 27：Device Control and Data Access；旧版：辅助功能。\n用于读取与切换窗口。授权后重新呼出搜索面板即可。",
                        "Enable Winlane in System Settings → Privacy & Security.\nmacOS 27: Device Control and Data Access; earlier: Accessibility.\nThis allows reading and switching windows. Reopen the panel after granting access."
                    ),
                )
            } else {
                (
                    if !switching && !state.query.borrow().trim().is_empty() {
                        tr!(
                            "没有匹配的窗口、应用或命令",
                            "No matching windows, apps, or commands"
                        )
                    } else {
                        tr!("没有匹配的窗口", "No matching windows")
                    },
                    tr!(
                        "试试其他关键词，或检查当前应用筛选和排除设置。",
                        "Try other keywords, or check the current-app filter and excluded apps."
                    ),
                )
            };
            let message_top = ((list_height - 142.0) / 2.0).max(12.0);
            let heading = label(
                title,
                21.0,
                rect(40.0, message_top, 540.0, 38.0),
                self.mtm(),
            );
            let detail = label(
                detail,
                14.0,
                rect(40.0, message_top + 42.0, 550.0, 96.0),
                self.mtm(),
            );
            detail.setTextColor(Some(&NSColor::secondaryLabelColor()));
            detail.setMaximumNumberOfLines(4);
            list.addSubview(&heading);
            list.addSubview(&detail);
            ui.empty_labels.replace(vec![heading, detail]);
        }
        for position in 0..count {
            let content = if let Some(project) = project_matches.get(position) {
                RowContent::Project(project.clone())
            } else if let Some(&command) = command_matches.get(position) {
                RowContent::Command(command)
            } else if let Some(snippet) = snippet_matches.get(position - command_matches.len()) {
                RowContent::Snippet(snippet.clone())
            } else if let Some(&id) =
                clipboard_matches.get(position - command_matches.len() - snippet_matches.len())
            {
                let entry = clipboard
                    .as_ref()
                    .and_then(|clipboard| clipboard.history.get(id))
                    .expect("matched history entry exists");
                let (source, preview, tooltip) = crate::macos::platform::clipboard::summary(entry);
                RowContent::Clipboard(id, source, preview, tooltip)
            } else if let Some(&index) = matched.get(position - extra_count) {
                let item = &windows[index];
                RowContent::Window(item.clone(), aliases.for_window(item.id).map(str::to_owned))
            } else if let Some(link) = quicklink_matches.get(position - extra_count - matched.len())
            {
                RowContent::Quicklink(link.clone())
            } else {
                RowContent::Application(
                    apps[launch_matches
                        [position - extra_count - matched.len() - quicklink_matches.len()]]
                    .target
                    .clone(),
                )
            };
            let selected = position == state.selected.get() && !unmatched_alias;
            if rows.len() <= position {
                rows.push(self.create_row(position, density));
            }
            let row = &mut rows[position];
            if row.content.as_ref() != Some(&content) {
                let tooltip = match &content {
                    RowContent::Command(id) => {
                        let command = id.definition();
                        set_label(&row.title, command.title());
                        set_label(&row.app, command.name);
                        set_label(&row.alias, ">_");
                        row.icon.setImage(
                            NSImage::imageWithSystemSymbolName_accessibilityDescription(
                                &NSString::from_str(command.symbol),
                                Some(&NSString::from_str(command.category())),
                            )
                            .as_deref(),
                        );
                        format!(
                            "{} — {} · {}",
                            command.name,
                            command.title(),
                            command.category()
                        )
                    }
                    RowContent::Project(project) => {
                        let path = project.path.to_string_lossy();
                        set_label(&row.app, &project.name);
                        set_label(&row.title, &path);
                        set_label(&row.alias, "↗");
                        let symbol = if project.kind == winlane::features::projects::Kind::Workspace
                        {
                            "rectangle.stack"
                        } else {
                            "folder"
                        };
                        row.icon.setImage(
                            NSImage::imageWithSystemSymbolName_accessibilityDescription(
                                &NSString::from_str(symbol),
                                Some(&NSString::from_str(tr!("VS Code 项目", "VS Code project"))),
                            )
                            .as_deref(),
                        );
                        format!("{} — {}", project.name, path)
                    }
                    RowContent::Quicklink(link) => {
                        set_label(&row.app, &link.name);
                        set_label(
                            &row.title,
                            &trf!("打开链接 · {}", "Open link · {}", link.link),
                        );
                        set_label(&row.alias, "↗");
                        row.icon.setImage(
                            NSImage::imageWithSystemSymbolName_accessibilityDescription(
                                &NSString::from_str("link"),
                                Some(&NSString::from_str(tr!("快捷链接", "Quicklink"))),
                            )
                            .as_deref(),
                        );
                        trf!("打开链接：{}", "Open link: {}", link.link)
                    }
                    RowContent::Snippet(snippet) => {
                        set_label(&row.app, &snippet.name);
                        let preview = snippet.body.lines().next().unwrap_or("");
                        set_label(
                            &row.title,
                            &trf!("粘贴片段 · {}", "Paste snippet · {}", preview),
                        );
                        set_label(&row.alias, "{}");
                        row.icon.setImage(
                            NSImage::imageWithSystemSymbolName_accessibilityDescription(
                                ns_string!("text.quote"),
                                Some(&NSString::from_str(tr!("文本片段", "Snippet"))),
                            )
                            .as_deref(),
                        );
                        trf!("粘贴片段：{}", "Paste snippet: {}", snippet.name)
                    }
                    RowContent::Clipboard(id, source, preview, tooltip) => {
                        set_label(&row.app, source);
                        set_label(&row.title, preview);
                        set_label(&row.alias, "↵");
                        let thumbnail = clipboard.as_ref().and_then(|clipboard| {
                            clipboard
                                .history
                                .get(*id)
                                .and_then(|entry| entry.image.as_ref())
                                .and_then(|image| clipboard.thumbnail(image))
                        });
                        row.icon.setImage(
                            thumbnail
                                .or_else(|| {
                                    NSImage::imageWithSystemSymbolName_accessibilityDescription(
                                        ns_string!("clipboard"),
                                        Some(&NSString::from_str(tr!(
                                            "剪贴板历史",
                                            "Clipboard history"
                                        ))),
                                    )
                                })
                                .as_deref(),
                        );
                        tooltip.clone()
                    }
                    RowContent::Window(item, alias) => {
                        let title = if item.title.trim().is_empty() {
                            &item.app
                        } else {
                            &item.title
                        };
                        let tooltip = format!(
                            "{} — {}{}",
                            item.app,
                            title,
                            alias
                                .as_deref()
                                .map_or(String::new(), |alias| format!(" · alias {alias}"))
                        );
                        let identities = state.identities.borrow();
                        let title = window_display_title(
                            item,
                            identities.get(&item.pid).map(|app| app.id.as_str()),
                        );
                        let title = if item.minimized {
                            trf!("{title} · 已最小化", "{title} · Minimized")
                        } else {
                            title.into_owned()
                        };
                        set_label(&row.title, &title);
                        set_label(&row.app, &item.app);
                        set_label(&row.alias, alias.as_deref().unwrap_or(""));
                        row.icon.setImage(self.cached_icon(item.pid).as_deref());
                        tooltip
                    }
                    RowContent::Application(app) => {
                        set_label(&row.title, tr!("启动应用", "Launch app"));
                        set_label(&row.app, &app.name);
                        set_label(&row.alias, "↗");
                        row.icon.setImage(Some(&self.application_icon(app)));
                        trf!("启动 {} — {}", "Launch {} — {}", app.name, app.path)
                    }
                };
                row.button.setToolTip(Some(&NSString::from_str(&tooltip)));
                row.button
                    .setAccessibilityLabel(Some(&NSString::from_str(&tooltip)));
                row.content = Some(content);
                row.selected = None;
            }
            if let Some(input) = quicklink_input.as_ref() {
                let preview = input.error.clone().unwrap_or_else(|| {
                    input
                        .destination()
                        .unwrap_or_else(|_| input.link.link.clone())
                });
                set_label(&row.title, &preview);
                row.button.setToolTip(Some(&NSString::from_str(&preview)));
                row.button
                    .setAccessibilityLabel(Some(&NSString::from_str(&format!(
                        "{} — {}",
                        input.link.name, preview
                    ))));
            }
            row.select(selected);
            if !row.attached {
                list.addSubview(&row.button);
                row.attached = true;
            }
            if selected {
                row.button.scrollRectToVisible(row.button.bounds());
            }
        }

        let clipboard_error = clipboard
            .as_ref()
            .and_then(|clipboard| clipboard.error.as_ref());
        let status = if inline {
            tr!(
                "Tab 切换参数 · ↵ 打开 · Esc 返回",
                "Tab next field · ↵ open · Esc back"
            )
            .into()
        } else if in_projects {
            project_cache.error.clone().unwrap_or_else(|| {
                if state.project_receiver.borrow().is_some() {
                    tr!("正在更新项目…", "Updating projects…").into()
                } else {
                    trf!(
                        "{} 个项目 · ↵ 用 VS Code 打开 · ⌘R 刷新 · Esc 返回",
                        "{} projects · ↵ open in VS Code · ⌘R refresh · Esc back",
                        project_matches.len()
                    )
                }
            })
        } else if in_clipboard && let Some(error) = clipboard_error {
            error.clone()
        } else if demo {
            trf!(
                "演示模式 · {} 个示例 · ↑↓ 选择  ↵ 预览选择  Esc 关闭",
                "Demo · {} examples · ↑↓ select · ↵ preview · Esc close",
                matched.len()
            )
        } else if let Some(error) = state.hotkey_error.borrow().as_ref() {
            error.clone()
        } else if let Some(error) = state.alias_error.borrow().as_ref() {
            error.clone()
        } else if !trusted {
            tr!(
                "需要辅助功能权限；也可以先查看演示。",
                "Accessibility access required. You can also try the demo."
            )
            .into()
        } else if in_clipboard {
            trf!(
                "{} · ↵ 粘贴 · ⌘C 复制 · ⌘⌫ 删除 · Esc 返回",
                "{} · ↵ paste · ⌘C copy · ⌘⌫ delete · Esc back",
                winlane::features::clipboard::entry_count(
                    clipboard_matches.len(),
                    !state.config.borrow().clipboard.enabled
                )
            )
        } else if self.searching_quicklinks() {
            trf!(
                "{} 个链接 · ↑↓ 选择 · ↵ 打开 · Esc 返回",
                "{} links · ↑↓ select · ↵ open · Esc back",
                quicklink_matches.len()
            )
        } else if snippets {
            trf!(
                "{} 个片段 · ↑↓ 选择 · ↵ 粘贴 · Esc 返回",
                "{} snippets · ↑↓ select · ↵ paste · Esc back",
                snippet_matches.len()
            )
        } else if state.loading.get() {
            tr!("正在刷新窗口…", "Refreshing windows…").into()
        } else if switching {
            trf!(
                "{} 项 · 字母定位 · ↑↓ 选择 · Space 搜索 · Esc 取消",
                "{} items · Type alias · ↑↓ select · Space search · Esc cancel",
                matched.len()
            )
        } else if !command_matches.is_empty() {
            trf!(
                "{} 项 · ↑↓ 选择 · ↵ 执行 / 打开 · Esc 取消",
                "{} items · ↑↓ select · ↵ run / open · Esc cancel",
                count
            )
        } else if !launch_matches.is_empty() {
            trf!(
                "{} 个窗口 · {} 个应用 · ↵ 切换 / 启动 · Esc 取消",
                "{} windows · {} apps · ↵ switch / launch · Esc cancel",
                matched.len(),
                launch_matches.len()
            )
        } else {
            trf!(
                "{} 项 · 输入 alias 或标题 · ↵ 切换 · Space 切换模式",
                "{} items · Type alias or title · ↵ switch · Space changes mode",
                matched.len()
            )
        };
        set_label(&ui.footer, &status);
        let has_error = !demo
            && (state.hotkey_error.borrow().is_some()
                || state.alias_error.borrow().is_some()
                || !trusted
                || (in_clipboard && clipboard_error.is_some())
                || (in_projects && project_cache.error.is_some()));
        ui.footer.setHidden(!show_hints && !has_error);
    }

    pub(super) fn create_row(&self, position: usize, density: DisplayDensity) -> RowUi {
        let mtm = self.mtm();
        let normal = density == DisplayDensity::Normal;
        let row_height = row_height(density);
        let frame = rect(
            2.0,
            position as f64 * row_height + 1.0,
            LIST_WIDTH - 4.0,
            row_height - 2.0,
        );
        // SAFETY: WindowRowButton inherits NSButton's designated frame initializer.
        let button: Retained<WindowRowButton> = unsafe {
            msg_send![super(WindowRowButton::alloc(mtm).set_ivars(RowAppearance::default())), initWithFrame: frame]
        };
        button.ivars().density.set(density);
        button.setTitle(ns_string!(""));
        button.setBordered(false);
        button.setTag(position as isize);
        // SAFETY: The delegate outlives its rows and pickWindow: takes a button sender.
        unsafe {
            button.setTarget(Some(self));
            button.setAction(Some(sel!(pickWindow:)));
        }
        let (alias_width, _) = alias_badge_size(density);
        let app_x = alias_width + 14.0;
        let app_width = if normal { 156.0 } else { 142.0 };
        let icon_x = app_x + app_width + 8.0;
        let icon_size = if normal { 24.0 } else { 20.0 };
        let title_x = icon_x + icon_size + 10.0;
        let title = label(
            "",
            if normal { 15.0 } else { 13.0 },
            rect(title_x, 0.0, LIST_WIDTH - title_x - 14.0, 0.0),
            mtm,
        );
        let app = label("", 13.0, rect(app_x, 0.0, app_width, 0.0), mtm);
        app.setFont(Some(&NSFont::systemFontOfSize_weight(
            if normal { 14.0 } else { 12.0 },
            unsafe { NSFontWeightMedium },
        )));
        app.setAlignment(NSTextAlignment::Right);
        for field in [&title, &app] {
            field.setMaximumNumberOfLines(1);
            field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
            let mut text_frame = field.frame();
            text_frame.size.height = field.intrinsicContentSize().height;
            text_frame.origin.y = (frame.size.height - text_frame.size.height) / 2.0;
            field.setFrame(text_frame);
            button.addSubview(field);
        }
        let alias = label("", 10.0, NSRect::ZERO, mtm);
        alias.setAlignment(NSTextAlignment::Center);
        alias.setFont(Some(&NSFont::monospacedSystemFontOfSize_weight(
            if normal { 11.0 } else { 10.0 },
            unsafe { NSFontWeightSemibold },
        )));
        let alias_height = alias.intrinsicContentSize().height;
        alias.setFrame(rect(
            8.0,
            (frame.size.height - alias_height) / 2.0,
            alias_width,
            alias_height,
        ));
        button.addSubview(&alias);
        let icon = NSImageView::initWithFrame(
            NSImageView::alloc(mtm),
            rect(
                icon_x,
                (frame.size.height - icon_size) / 2.0,
                icon_size,
                icon_size,
            ),
        );
        icon.setImageScaling(NSImageScaling::ScaleProportionallyDown);
        button.addSubview(&icon);
        RowUi {
            button,
            title,
            app,
            alias,
            icon,
            content: None,
            selected: None,
            attached: false,
        }
    }
}

pub(super) fn window_display_title<'a>(
    window: &'a WindowInfo,
    app_id: Option<&str>,
) -> Cow<'a, str> {
    if window.title.trim().is_empty() {
        return Cow::Borrowed(&window.app);
    }
    let is_vscode = match app_id {
        Some(id) => matches!(id, "com.microsoft.VSCode" | "com.microsoft.VSCodeInsiders"),
        None => matches!(
            window.app.as_str(),
            "Code" | "Visual Studio Code" | "Code - Insiders"
        ),
    };
    if !is_vscode {
        return Cow::Borrowed(&window.title);
    }
    let split = |title: &'a str| {
        title
            .rsplit_once(" — ")
            .or_else(|| title.rsplit_once(" - "))
    };
    let title = [
        "Visual Studio Code - Insiders",
        "Visual Studio Code",
        "Code - Insiders",
        "Code",
    ]
    .into_iter()
    .find_map(|name| {
        let title = window.title.strip_suffix(name)?;
        title
            .strip_suffix(" — ")
            .or_else(|| title.strip_suffix(" - "))
    })
    .unwrap_or(&window.title);
    let Some((detail, project)) = split(title) else {
        return Cow::Borrowed(&window.title);
    };
    if detail.trim().is_empty() || project.trim().is_empty() {
        return Cow::Borrowed(&window.title);
    }
    Cow::Owned(format!("{}: {}", project.trim(), detail.trim()))
}
