use super::*;
use crate::macos::platform::applications::target_at_url;
use block2::RcBlock;
use objc2_foundation::{NSArray, NSURL, ns_string};
use std::cell::Cell;
use std::rc::Rc;
use winlane::core::key_remap::{
    KeyRemap, MAX_REMAPS, REMOTE_DESKTOP_APPS, TERMINAL_APPS, format_combination, parse_combination,
};

/// One rule per list entry: what to press, what to send, and who to leave
/// alone. The combinations are text because a rule has to be able to name any
/// key, including the ones Winlane's own shortcut controls cannot pick.
const ROW_HEIGHT: f64 = 58.0;
const ROW_GAP: f64 = 6.0;
const APP_ROW: f64 = 20.0;
/// Title, description, status line and the add button above the first row.
const CARD_HEADER: f64 = 140.0;
const CARD_PADDING: f64 = 8.0;
/// A row and one of its excluded applications travel in one button tag, which
/// is how AppKit reports which control was used.
const TAG_STRIDE: isize = 1024;
/// The popup's first item opens the application panel; the rest are lists.
const ADD_TERMINALS: isize = 1;
const ADD_REMOTE: isize = 2;

struct ExclusionEntry {
    view: Retained<NSView>,
    remove: Retained<NSButton>,
}

struct KeyRemapRow {
    view: Retained<NSView>,
    enabled: Retained<NSButton>,
    extra: Retained<NSButton>,
    from: Retained<NSTextField>,
    to: Retained<NSTextField>,
    remove: Retained<NSButton>,
    summary: Retained<NSButton>,
    add: Retained<NSPopUpButton>,
    except_label: Retained<NSTextField>,
    arrow: Retained<NSTextField>,
    divider: Retained<NSBox>,
    exclusions: RefCell<Vec<String>>,
    entries: RefCell<Vec<ExclusionEntry>>,
    expanded: Cell<bool>,
    /// What the row was filled with, so an untouched rule keeps the exact
    /// spelling it was saved with instead of being rewritten by a round trip
    /// through the text fields.
    original: KeyRemap,
}

impl KeyRemapRow {
    /// Collapsed rows are one line; an expanded list adds one line per
    /// application, so a rule excluding thirty of them does not bury the rules
    /// below it.
    fn height(&self) -> f64 {
        if self.expanded.get() {
            ROW_HEIGHT + 4.0 + self.entries.borrow().len() as f64 * APP_ROW
        } else {
            ROW_HEIGHT
        }
    }

    /// Place the row's controls for its current height. The two fixed lines stay
    /// at the top, so a growing list extends downwards.
    fn position(&self, height: f64) {
        // The two fixed lines keep the top `ROW_HEIGHT` points of the row; an
        // expanded list fills the extra space below them.
        let base = height - ROW_HEIGHT;
        let top = height - 32.0;
        self.enabled.setFrame(rect(0.0, top + 2.0, 24.0, 24.0));
        self.from.setFrame(rect(32.0, top, 170.0, 26.0));
        self.to.setFrame(rect(238.0, top, 170.0, 26.0));
        self.extra.setFrame(rect(418.0, top + 2.0, 180.0, 24.0));
        self.remove.setFrame(rect(604.0, top, 56.0, 26.0));
        self.arrow.setFrameOrigin(NSPoint::new(210.0, top + 2.0));
        self.except_label
            .setFrameOrigin(NSPoint::new(32.0, base + 4.0));
        self.divider
            .setFrameOrigin(NSPoint::new(20.0, height - 1.0));
        self.summary.setFrame(rect(106.0, base + 2.0, 240.0, 22.0));
        self.add.setFrame(rect(354.0, base + 2.0, 180.0, 22.0));
        if self.expanded.get() {
            for (index, entry) in self.entries.borrow().iter().enumerate() {
                entry.view.setFrame(rect(
                    106.0,
                    base - (index as f64 + 1.0) * APP_ROW,
                    380.0,
                    APP_ROW - 2.0,
                ));
            }
        }
    }

    fn exclusions(&self) -> Vec<String> {
        self.exclusions.borrow().clone()
    }

    fn add_exclusions(&self, bundle_ids: &[&str]) {
        {
            let mut exclusions = self.exclusions.borrow_mut();
            for bundle_id in bundle_ids {
                if !exclusions.iter().any(|existing| existing == bundle_id) {
                    exclusions.push((*bundle_id).to_string());
                }
            }
        }
        self.refresh_exclusions();
    }

    fn remove_exclusion(&self, index: usize) -> bool {
        {
            let mut exclusions = self.exclusions.borrow_mut();
            if index >= exclusions.len() {
                return false;
            }
            exclusions.remove(index);
        }
        self.refresh_exclusions();
        true
    }

    fn toggle_expanded(&self) {
        if self.exclusions.borrow().is_empty() {
            return;
        }
        self.expanded.set(!self.expanded.get());
        // The entries only belong in the view while the list is open, so a
        // collapsed rule is exactly one line tall.
        self.refresh_exclusions();
    }

    /// Rebuild the application entries and the summary from the current list.
    fn refresh_exclusions(&self) {
        let mtm = self.view.mtm();
        let target = self.add.target().expect("settings target is alive");
        for entry in self.entries.borrow_mut().drain(..) {
            entry.view.removeFromSuperview();
        }
        let bundle_ids = self.exclusions.borrow().clone();
        let mut entries = Vec::with_capacity(bundle_ids.len());
        for bundle_id in &bundle_ids {
            let (name, icon, path) = excluded_application(bundle_id);
            let view =
                NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 380.0, APP_ROW - 2.0));
            if let Some(icon) = icon {
                let image =
                    NSImageView::initWithFrame(NSImageView::alloc(mtm), rect(0.0, 3.0, 14.0, 14.0));
                image.setImage(Some(&icon));
                image.setImageScaling(NSImageScaling::ScaleProportionallyDown);
                view.addSubview(&image);
            }
            let name_label = label(&name, 12.0, rect(20.0, 1.0, 316.0, 16.0), mtm);
            name_label.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
            view.addSubview(&name_label);
            let remove = button(
                "×",
                &target,
                sel!(removeKeyRemapExclusion:),
                rect(340.0, 0.0, 24.0, 18.0),
                mtm,
            );
            remove.setBordered(false);
            remove.setToolTip(Some(&NSString::from_str(&trf!(
                "不再排除 {name}",
                "Stop excluding {name}",
                name = name
            ))));
            view.addSubview(&remove);
            let tooltip = if path.is_empty() {
                bundle_id.clone()
            } else {
                format!("{bundle_id}\n{path}")
            };
            view.setToolTip(Some(&NSString::from_str(&tooltip)));
            if self.expanded.get() {
                self.view.addSubview(&view);
            }
            entries.push(ExclusionEntry { view, remove });
        }
        let count = bundle_ids.len();
        *self.entries.borrow_mut() = entries;
        self.summary.setTitle(&NSString::from_str(&if count == 0 {
            tr!("未排除任何应用", "No excluded apps").to_string()
        } else {
            trf!(
                "排除 {count} 个应用 {mark}",
                "{count} excluded app(s) {mark}",
                count = count,
                mark = if self.expanded.get() { "▾" } else { "▸" }
            )
        }));
        self.summary.setEnabled(count > 0);
        self.position(self.height());
    }

    /// Which row and application a control belongs to, from its tag.
    fn update_tags(&self, row: usize) {
        self.remove.setTag(row as isize);
        self.add.setTag(row as isize);
        self.summary.setTag(row as isize);
        for (index, entry) in self.entries.borrow().iter().enumerate() {
            entry
                .remove
                .setTag((row as isize) * TAG_STRIDE + index as isize);
        }
    }
}

/// The name, icon and path of an excluded application. An application that is
/// not installed still shows as its bundle identifier.
fn excluded_application(bundle_id: &str) -> (String, Option<Retained<NSImage>>, String) {
    let workspace = NSWorkspace::sharedWorkspace();
    let Some(url) = workspace.URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle_id))
    else {
        return (bundle_id.to_string(), None, String::new());
    };
    let Ok(application) = target_at_url(&url) else {
        return (bundle_id.to_string(), None, String::new());
    };
    let path = application.path.clone();
    let icon = workspace.iconForFile(&NSString::from_str(&path));
    (application.name, Some(icon), path)
}

pub(super) struct KeyRemapsPage {
    card: Retained<NSView>,
    title: Retained<NSTextField>,
    description: Retained<NSTextField>,
    status: Retained<NSTextField>,
    enabled: Retained<NSButton>,
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
        let enabled = checkbox(tr!("启用按键重映射", "Enable key remaps"), mtm);
        enabled.setFrame(rect(40.0, CARD_HEADER - 106.0, 300.0, 22.0));
        enabled.setToolTip(Some(&NSString::from_str(tr!(
            "关掉后所有规则暂时停用，规则本身保留在列表里",
            "Turning this off suspends every rule; the rules themselves stay in the list"
        ))));
        set_action(&enabled, target, sel!(settingsChanged:));
        card.addSubview(&enabled);
        let status = hint("", rect(40.0, CARD_HEADER - 130.0, 660.0, 18.0), mtm);
        card.addSubview(&status);
        Self {
            card,
            title,
            description,
            status,
            enabled,
            add,
            rows: RefCell::default(),
        }
    }

    /// The card height, so the page that owns the scroll view can lay this out.
    pub(super) fn height(&self) -> f64 {
        CARD_HEADER
            + CARD_PADDING * 2.0
            + self
                .rows
                .borrow()
                .iter()
                .map(|row| row.height() + ROW_GAP)
                .sum::<f64>()
    }

    /// Place the card with its bottom at `y`; rows grow downward from the top.
    pub(super) fn layout(&self, y: f64) {
        let rows = self.rows.borrow();
        let height = self.height();
        self.card.setFrame(rect(0.0, y, 740.0, height));
        self.title.setFrameOrigin(NSPoint::new(40.0, height - 36.0));
        self.description
            .setFrameOrigin(NSPoint::new(40.0, height - 78.0));
        self.enabled
            .setFrameOrigin(NSPoint::new(40.0, height - 106.0));
        self.status
            .setFrameOrigin(NSPoint::new(40.0, height - 130.0));
        self.add.setFrameOrigin(NSPoint::new(520.0, height - 64.0));
        let mut top = height - CARD_HEADER;
        for (index, row) in rows.iter().enumerate() {
            let row_height = row.height();
            row.view
                .setFrame(rect(40.0, top - row_height, 660.0, row_height));
            row.update_tags(index);
            top -= row_height + ROW_GAP;
        }
        self.add.setEnabled(rows.len() + 1 < MAX_REMAPS);
        let paused = if self.enabled.state() == NSControlStateValueOn {
            ""
        } else {
            tr!(" · 已停用", " · paused")
        };
        if rows.is_empty() {
            self.status.setStringValue(&NSString::from_str(tr!(
                "还没有规则。用“添加规则”新建，或用 scripts/migrate-karabiner.py 从 Karabiner 迁移。",
                "No rules yet. Add one here, or migrate Karabiner rules with scripts/migrate-karabiner.py."
            )));
        } else {
            self.status.setStringValue(&NSString::from_str(&trf!(
                "共 {count} 条规则，修改后自动保存。{paused}",
                "{count} rules; changes save automatically.{paused}",
                count = rows.len(),
                paused = paused
            )));
        }
    }

    pub(super) fn fill(&self, config: &Config) {
        self.enabled.setState(if config.key_remaps_enabled {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
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
        config.key_remaps_enabled = self.enabled.state() == NSControlStateValueOn;
        let rows = self.rows.borrow();
        let mut remaps = Vec::with_capacity(rows.len());
        for (index, row) in rows.iter().enumerate() {
            let position = index + 1;
            let from = row.from.stringValue().to_string();
            let to_text = row.to.stringValue().to_string();
            let untouched =
                format_combination(&row.original.from_key, &row.original.from_modifiers) == from
                    && format_combination(&row.original.to_key, &row.original.to_modifiers)
                        == to_text
                    && row.original.except_apps == row.exclusions();
            let (from_key, from_modifiers) = if untouched {
                (
                    row.original.from_key.clone(),
                    row.original.from_modifiers.clone(),
                )
            } else {
                parse_combination(&from).map_err(|error| {
                    trf!(
                        "第 {position} 条规则的“按下”无效：{error}",
                        "Rule {position}: the pressed combination is invalid. {error}",
                        position = position,
                        error = error
                    )
                })?
            };
            let (to_key, to_modifiers) = if untouched {
                (
                    row.original.to_key.clone(),
                    row.original.to_modifiers.clone(),
                )
            } else {
                parse_combination(&to_text).map_err(|error| {
                    trf!(
                        "第 {position} 条规则的“发出”无效：{error}",
                        "Rule {position}: the sent combination is invalid. {error}",
                        position = position,
                        error = error
                    )
                })?
            };
            remaps.push(KeyRemap {
                id: row.original.id.clone(),
                enabled: row.enabled.state() == NSControlStateValueOn,
                allow_extra_modifiers: row.extra.state() == NSControlStateValueOn,
                from_key,
                from_modifiers,
                to_key,
                to_modifiers,
                except_apps: row.exclusions(),
            });
        }
        config.key_remaps = remaps;
        Ok(())
    }

    fn append_row(&self, remap: &KeyRemap) -> Rc<KeyRemapRow> {
        let mtm = self.card.mtm();
        let target = self.add.target().expect("settings target is alive");
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 660.0, ROW_HEIGHT));
        let divider = row_divider(&view, ROW_HEIGHT - 1.0, mtm);

        let enabled = checkbox("", mtm);
        set_action(&enabled, &target, sel!(settingsChanged:));
        enabled.setToolTip(Some(&NSString::from_str(tr!(
            "启用这条规则",
            "Apply this rule"
        ))));
        enabled.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "启用这条规则",
            "Apply this rule"
        ))));
        view.addSubview(&enabled);

        let from = input(rect(0.0, 0.0, 170.0, 26.0), tr!("⌃S", "⌃S"), mtm);
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

        let arrow = label("→", 14.0, rect(210.0, 30.0, 20.0, 22.0), mtm);
        view.addSubview(&arrow);

        let to = input(rect(238.0, 0.0, 170.0, 26.0), "⌘S", mtm);
        to.setStringValue(&NSString::from_str(&format_combination(
            &remap.to_key,
            &remap.to_modifiers,
        )));
        to.setToolTip(Some(&NSString::from_str(tr!(
            "发出的组合；不带修饰键就是该按键本身",
            "The combination to send; no modifiers means the bare key"
        ))));
        set_action(&to, &target, sel!(settingsChanged:));
        view.addSubview(&to);

        let extra = checkbox(tr!("允许额外修饰键", "Allow extra modifiers"), mtm);
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
            rect(604.0, 26.0, 56.0, 26.0),
            mtm,
        );
        view.addSubview(&remove);

        let except_label = hint(
            tr!("排除应用", "Excluded apps"),
            rect(32.0, 4.0, 74.0, 18.0),
            mtm,
        );
        view.addSubview(&except_label);
        let summary = button(
            "",
            &target,
            sel!(toggleKeyRemapExclusions:),
            rect(106.0, 0.0, 240.0, 22.0),
            mtm,
        );
        summary.setBordered(false);
        summary.setContentTintColor(Some(&NSColor::linkColor()));
        view.addSubview(&summary);

        // The application panel the input-rule and Auto AppClose pages already
        // use, plus the two lists a migrated Karabiner rule usually needs.
        let add = popup(
            &[
                tr!("＋ 添加应用…", "＋ Add App…"),
                tr!("排除终端类应用", "Exclude terminals"),
                tr!("排除远程桌面与虚拟机", "Exclude remote desktops"),
            ],
            rect(0.0, 0.0, 180.0, 22.0),
            mtm,
        );
        set_action(&add, &target, sel!(addKeyRemapExclusions:));
        add.setToolTip(Some(&NSString::from_str(tr!(
            "从应用程序里挑选，或一次加入常用的排除列表",
            "Pick one application, or add the usual exclusion lists at once"
        ))));
        view.addSubview(&add);

        self.card.addSubview(&view);
        let row = Rc::new(KeyRemapRow {
            view,
            enabled,
            extra,
            from,
            to,
            remove,
            summary,
            add,
            except_label,
            arrow,
            divider,
            exclusions: RefCell::default(),
            entries: RefCell::default(),
            expanded: Cell::new(false),
            original: remap.clone(),
        });
        if remap.except_apps.is_empty() {
            row.refresh_exclusions();
        } else {
            row.add_exclusions(
                &remap
                    .except_apps
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            );
        }
        row.position(ROW_HEIGHT);
        self.rows.borrow_mut().push(row.clone());
        row
    }

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

    /// One of the exclusion lists, chosen from a rule's popup.
    pub(super) fn add_exclusions(&self, row: usize, choice: isize) {
        let Some(row) = self.rows.borrow().get(row).cloned() else {
            return;
        };
        match choice {
            ADD_TERMINALS => row.add_exclusions(TERMINAL_APPS),
            ADD_REMOTE => row.add_exclusions(REMOTE_DESKTOP_APPS),
            _ => {}
        }
    }

    pub(super) fn toggle_exclusions(&self, row: usize) {
        if let Some(row) = self.rows.borrow().get(row) {
            row.toggle_expanded();
        }
    }

    /// Remove one excluded application; the tag carries the row in its high
    /// bits and the application in its low bits.
    pub(super) fn remove_exclusion_for_tag(&self, tag: isize) -> bool {
        let (row, index) = ((tag / TAG_STRIDE) as usize, (tag % TAG_STRIDE) as usize);
        let rows = self.rows.borrow();
        rows.get(row).is_some_and(|row| row.remove_exclusion(index))
    }

    /// Ask for an application the same way the other app-list settings do, and
    /// add it to the rule's exclusion list instead of to a row of its own.
    pub(super) fn choose_application(
        &self,
        row: usize,
        window: &NSWindow,
        message: &Retained<NSTextField>,
    ) {
        let Some(target_row) = self.rows.borrow().get(row).cloned() else {
            return;
        };
        let panel = NSOpenPanel::openPanel(window.mtm());
        panel.setCanChooseFiles(true);
        panel.setCanChooseDirectories(false);
        panel.setAllowsMultipleSelection(false);
        panel.setTreatsFilePackagesAsDirectories(false);
        #[allow(deprecated)]
        panel.setAllowedFileTypes(Some(&NSArray::from_slice(&[ns_string!("app")])));
        panel.setDirectoryURL(Some(&NSURL::fileURLWithPath(ns_string!("/Applications"))));
        panel.setPrompt(Some(&NSString::from_str(tr!(
            "排除此应用",
            "Exclude This App"
        ))));
        let selected = panel.clone();
        let message = message.clone();
        let completion = RcBlock::new(move |response| {
            if response != NSModalResponseOK || target_row.view.window().is_none() {
                return;
            }
            let Some(url) = selected.URL() else {
                return;
            };
            match target_at_url(&url) {
                Ok(application) => {
                    let name = application.name.clone();
                    if target_row
                        .exclusions
                        .borrow()
                        .iter()
                        .any(|existing| existing == &application.bundle_id)
                    {
                        message.setStringValue(&NSString::from_str(&trf!(
                            "{name} 已在排除列表中。",
                            "{name} is already excluded.",
                            name = name
                        )));
                        message.setTextColor(Some(&NSColor::systemRedColor()));
                        return;
                    }
                    target_row.add_exclusions(&[&application.bundle_id]);
                    // The list may have grown: ask the settings window to lay
                    // the page out again and save the change.
                    let control = &target_row.summary;
                    unsafe {
                        control.sendAction_to(
                            Some(sel!(remapExclusionsChanged:)),
                            control.target().as_deref(),
                        );
                    }
                }
                Err(error) => {
                    message.setStringValue(&NSString::from_str(&error));
                    message.setTextColor(Some(&NSColor::systemRedColor()));
                }
            }
        });
        panel.beginSheetModalForWindow_completionHandler(window, &completion);
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

    #[cfg(test)]
    #[allow(dead_code)]
    pub(super) fn set_enabled(&self, enabled: bool) {
        self.enabled.setState(if enabled {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
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
    pub(super) fn row_exclusions(&self, index: usize) -> Vec<String> {
        self.rows
            .borrow()
            .get(index)
            .map(|row| row.exclusions())
            .unwrap_or_default()
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(super) fn remove_row_exclusion(&self, row: usize, index: usize) -> bool {
        self.remove_exclusion_for_tag(row as isize * TAG_STRIDE + index as isize)
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
}
