use super::*;
use std::rc::Rc;
use winlane::core::key_remap::{KeyRemap, MAX_REMAPS, format_combination, parse_combination};

/// One rule per list entry: what to press, what to send, and who to leave
/// alone. The fields are text because a rule has to be able to name any key,
/// including the ones Winlane's own shortcut controls cannot pick.
const ROW_HEIGHT: f64 = 58.0;
const ROW_GAP: f64 = 6.0;
/// Title, description, status line and the add button above the first row.
const CARD_HEADER: f64 = 116.0;
const CARD_PADDING: f64 = 8.0;

struct KeyRemapRow {
    view: Retained<NSView>,
    enabled: Retained<NSButton>,
    extra: Retained<NSButton>,
    from: Retained<NSTextField>,
    to: Retained<NSTextField>,
    except: Retained<NSTextField>,
    remove: Retained<NSButton>,
    /// What the row was filled with, so an untouched rule keeps the exact
    /// spelling it was saved with instead of being rewritten by a round trip
    /// through the text fields.
    original: KeyRemap,
}

pub(super) struct KeyRemapsPage {
    card: Retained<NSView>,
    title: Retained<NSTextField>,
    description: Retained<NSTextField>,
    status: Retained<NSTextField>,
    add: Retained<NSButton>,
    rows: RefCell<Vec<Rc<KeyRemapRow>>>,
}

impl KeyRemapsPage {
    pub(super) fn new(document: &NSView, target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let card = card(document, rect(0.0, 0.0, 740.0, CARD_HEADER), mtm);
        let title = label(
            tr!("按键重映射", "Key remaps"),
            15.0,
            rect(40.0, CARD_HEADER - 36.0, 460.0, 24.0),
            mtm,
        );
        card.addSubview(&title);
        let description = hint(
            tr!(
                "“按下 → 发出”的替换，对所有应用生效；列出的应用保持原样。新增规则默认停用。",
                "Replace one combination with another in every application; listed applications keep the original. New rules start disabled."
            ),
            rect(40.0, CARD_HEADER - 78.0, 470.0, 38.0),
            mtm,
        );
        card.addSubview(&description);
        let add = button(
            tr!("＋ 添加规则", "＋ Add Rule"),
            target,
            sel!(addKeyRemap:),
            rect(520.0, CARD_HEADER - 64.0, 180.0, 28.0),
            mtm,
        );
        card.addSubview(&add);
        let status = hint("", rect(40.0, CARD_HEADER - 100.0, 660.0, 18.0), mtm);
        card.addSubview(&status);
        Self {
            card,
            title,
            description,
            status,
            add,
            rows: RefCell::default(),
        }
    }

    /// The card height, so the page that owns the scroll view can lay this out.
    pub(super) fn height(&self) -> f64 {
        CARD_HEADER + CARD_PADDING * 2.0 + self.rows.borrow().len() as f64 * (ROW_HEIGHT + ROW_GAP)
    }

    /// Place the card with its bottom at `y`; rows grow downward from the top.
    pub(super) fn layout(&self, y: f64) {
        let rows = self.rows.borrow();
        let height = self.height();
        self.card.setFrame(rect(0.0, y, 740.0, height));
        self.title.setFrameOrigin(NSPoint::new(40.0, height - 36.0));
        self.description
            .setFrameOrigin(NSPoint::new(40.0, height - 78.0));
        self.status
            .setFrameOrigin(NSPoint::new(40.0, height - 100.0));
        self.add.setFrameOrigin(NSPoint::new(520.0, height - 64.0));
        for (index, row) in rows.iter().enumerate() {
            row.view.setFrame(rect(
                40.0,
                height - CARD_HEADER - (index as f64 + 1.0) * (ROW_HEIGHT + ROW_GAP) + ROW_GAP,
                660.0,
                ROW_HEIGHT,
            ));
            row.remove.setTag(index as isize);
        }
        self.add
            .setEnabled(rows.len() < MAX_REMAPS && rows.len() + 1 < MAX_REMAPS);
        if rows.is_empty() {
            self.status.setStringValue(&NSString::from_str(tr!(
                "还没有规则。用“添加规则”新建，或用 scripts/migrate-karabiner.py 从 Karabiner 迁移。",
                "No rules yet. Add one here, or migrate Karabiner rules with scripts/migrate-karabiner.py."
            )));
        } else {
            self.status.setStringValue(&NSString::from_str(&trf!(
                "共 {count} 条规则，修改后自动保存。",
                "{count} rules; changes save automatically.",
                count = rows.len()
            )));
        }
    }

    pub(super) fn fill(&self, config: &Config) {
        for row in self.rows.borrow_mut().drain(..) {
            row.view.removeFromSuperview();
        }
        for remap in &config.key_remaps {
            let row = self.append_row(remap);
            row.enabled.setState(if remap.enabled {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
    }

    pub(super) fn read(&self, config: &mut Config) -> Result<(), String> {
        let rows = self.rows.borrow();
        let mut remaps = Vec::with_capacity(rows.len());
        for (index, row) in rows.iter().enumerate() {
            let position = index + 1;
            let from = row.from.stringValue().to_string();
            let to_text = row.to.stringValue().to_string();
            let except_text = row.except.stringValue().to_string();
            if format_combination(&row.original.from_key, &row.original.from_modifiers) == from
                && format_combination(&row.original.to_key, &row.original.to_modifiers) == to_text
                && row.original.except_apps.join(", ") == except_text
            {
                remaps.push(KeyRemap {
                    id: row.original.id.clone(),
                    enabled: row.enabled.state() == NSControlStateValueOn,
                    allow_extra_modifiers: row.extra.state() == NSControlStateValueOn,
                    ..row.original.clone()
                });
                continue;
            }
            let (from_key, from_modifiers) = parse_combination(&from).map_err(|error| {
                trf!(
                    "第 {position} 条规则的“按下”无效：{error}",
                    "Rule {position}: the pressed combination is invalid. {error}",
                    position = position,
                    error = error
                )
            })?;
            let to = row.to.stringValue().to_string();
            let (to_key, to_modifiers) = parse_combination(&to).map_err(|error| {
                trf!(
                    "第 {position} 条规则的“发出”无效：{error}",
                    "Rule {position}: the sent combination is invalid. {error}",
                    position = position,
                    error = error
                )
            })?;
            let except_apps = row
                .except
                .stringValue()
                .to_string()
                .split([',', '，', '\n', ' ', '\t'])
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect();
            remaps.push(KeyRemap {
                id: row.original.id.clone(),
                enabled: row.enabled.state() == NSControlStateValueOn,
                from_key,
                from_modifiers,
                allow_extra_modifiers: row.extra.state() == NSControlStateValueOn,
                to_key,
                to_modifiers,
                except_apps,
            });
        }
        config.key_remaps = remaps;
        Ok(())
    }

    fn append_row(&self, remap: &KeyRemap) -> Rc<KeyRemapRow> {
        let mtm = self.card.mtm();
        let target = self.add.target().expect("settings target is alive");
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 660.0, ROW_HEIGHT));
        row_divider(&view, ROW_HEIGHT - 1.0, mtm);

        let enabled = checkbox("", mtm);
        enabled.setFrame(rect(0.0, ROW_HEIGHT - 30.0, 24.0, 24.0));
        set_action(&enabled, &target, sel!(settingsChanged:));
        view.addSubview(&enabled);
        enabled.setToolTip(Some(&NSString::from_str(tr!(
            "启用这条规则",
            "Apply this rule"
        ))));
        enabled.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "启用这条规则",
            "Apply this rule"
        ))));

        let from = input(
            rect(32.0, ROW_HEIGHT - 32.0, 170.0, 26.0),
            tr!("⌃S", "⌃S"),
            mtm,
        );
        from.setStringValue(&NSString::from_str(&format_combination(
            &remap.from_key,
            &remap.from_modifiers,
        )));
        from.setToolTip(Some(&NSString::from_str(tr!(
            "按下的组合，例如 ⌃S、⌘⇧K、f7、left_arrow、page_up",
            "The combination to press, such as ⌃S, ⌘⇧K, f7, left_arrow or page_up"
        ))));
        set_action(&from, &target, sel!(settingsChanged:));
        view.addSubview(&from);

        let arrow = label("→", 14.0, rect(210.0, ROW_HEIGHT - 30.0, 20.0, 22.0), mtm);
        view.addSubview(&arrow);

        let to = input(rect(238.0, ROW_HEIGHT - 32.0, 170.0, 26.0), "⌘S", mtm);
        to.setStringValue(&NSString::from_str(&format_combination(
            &remap.to_key,
            &remap.to_modifiers,
        )));
        to.setToolTip(Some(&NSString::from_str(tr!(
            "发出的组合；留空表示不按修饰键",
            "The combination to send; no modifiers means the bare key"
        ))));
        set_action(&to, &target, sel!(settingsChanged:));
        view.addSubview(&to);

        let extra = checkbox(tr!("允许额外修饰键", "Allow extra modifiers"), mtm);
        extra.setFrame(rect(418.0, ROW_HEIGHT - 30.0, 180.0, 24.0));
        extra.setState(if remap.allow_extra_modifiers {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        extra.setToolTip(Some(&NSString::from_str(tr!(
            "勾选后，再按住其他修饰键也会触发；不勾选则必须完全一致（例如 End 不会影响 Shift+End）",
            "When on, holding other modifiers still matches; when off the combination has to match exactly (so End leaves Shift+End alone)"
        ))));
        set_action(&extra, &target, sel!(settingsChanged:));
        view.addSubview(&extra);

        let remove = button(
            tr!("删除", "Delete"),
            &target,
            sel!(removeKeyRemap:),
            rect(604.0, ROW_HEIGHT - 32.0, 56.0, 26.0),
            mtm,
        );
        view.addSubview(&remove);

        let except_label = hint(
            tr!("排除应用", "Excluded apps"),
            rect(32.0, 4.0, 100.0, 18.0),
            mtm,
        );
        view.addSubview(&except_label);
        let except = input(
            rect(136.0, 2.0, 524.0, 24.0),
            tr!(
                "com.apple.Terminal, com.googlecode.iterm2",
                "com.apple.Terminal, com.googlecode.iterm2"
            ),
            mtm,
        );
        except.setStringValue(&NSString::from_str(&remap.except_apps.join(", ")));
        except.setToolTip(Some(&NSString::from_str(tr!(
            "这些应用保持原样，用逗号分隔 bundle id",
            "These applications keep the original combination; separate bundle identifiers with commas"
        ))));
        set_action(&except, &target, sel!(settingsChanged:));
        view.addSubview(&except);

        self.card.addSubview(&view);
        let row = Rc::new(KeyRemapRow {
            view,
            enabled,
            extra,
            from,
            to,
            except,
            remove,
            original: remap.clone(),
        });
        self.rows.borrow_mut().push(row.clone());
        row
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(super) fn card_frame(&self) -> NSRect {
        self.card.frame()
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(super) fn row_count(&self) -> usize {
        self.rows.borrow().len()
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(super) fn set_row_from(&self, index: usize, text: &str) {
        if let Some(row) = self.rows.borrow().get(index) {
            row.from.setStringValue(&NSString::from_str(text));
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(super) fn set_row_enabled(&self, index: usize, enabled: bool) {
        if let Some(row) = self.rows.borrow().get(index) {
            row.enabled.setState(if enabled {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
    }

    /// Append a disabled placeholder rule; the caller lays the card out again.
    pub(super) fn add_rule(&self) {
        if self.rows.borrow().len() + 1 >= MAX_REMAPS {
            return;
        }
        let rule = self.blank_rule();
        self.append_row(&rule);
    }

    pub(super) fn remove_rule(&self, index: usize) {
        if index < self.rows.borrow().len() {
            self.rows
                .borrow_mut()
                .remove(index)
                .view
                .removeFromSuperview();
        }
    }

    /// A disabled placeholder: it cannot collide with an existing rule while it
    /// is being edited, and the checkbox right beside it turns it on.
    pub(super) fn blank_rule(&self) -> KeyRemap {
        KeyRemap {
            id: format!(
                "custom-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_millis())
                    .unwrap_or_default()
            ),
            enabled: false,
            from_key: "f12".into(),
            from_modifiers: vec!["command".into()],
            allow_extra_modifiers: true,
            to_key: "f12".into(),
            to_modifiers: Vec::new(),
            except_apps: Vec::new(),
        }
    }
}
