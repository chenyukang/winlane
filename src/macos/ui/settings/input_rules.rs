use super::*;
use crate::macos::platform::{applications::target_at_url, input_source::Source};
use block2::RcBlock;
use objc2_foundation::{NSArray, NSURL, ns_string};
use std::rc::Rc;
use winlane::core::config::ApplicationTarget;
use winlane::features::input_rules::{
    AppRule, InputSource, RestoreStrategy, SourceRule, WINLANE_ID,
};

struct RuleRow {
    view: Retained<NSView>,
    application: RefCell<Option<ApplicationTarget>>,
    choose: Retained<NSButton>,
    remove: Retained<NSButton>,
    source: SourcePicker,
    restore: Retained<NSPopUpButton>,
}

struct SourcePicker {
    control: Retained<NSPopUpButton>,
    values: RefCell<Vec<SourceRule>>,
    inherit: bool,
}

impl SourcePicker {
    fn new(frame: NSRect, inherit: bool, target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let control = popup(&[], frame, mtm);
        set_action(&control, target, sel!(settingsChanged:));
        Self {
            control,
            values: RefCell::default(),
            inherit,
        }
    }
    fn fill(&self, selected: SourceRule, sources: &[InputSource]) {
        let mut values = Vec::new();
        self.control.removeAllItems();
        if self.inherit {
            self.control.addItemWithTitle(&NSString::from_str(tr!(
                "使用全局设置",
                "Use global setting"
            )));
            values.push(SourceRule::Global);
        }
        self.control.addItemWithTitle(&NSString::from_str(tr!(
            "保持当前输入法",
            "Keep current input source"
        )));
        values.push(SourceRule::Current);
        for (value, title) in [
            (
                SourceRule::English,
                tr!("英文（自动选择）", "English (automatic)"),
            ),
            (
                SourceRule::Chinese,
                tr!("中文（自动选择）", "Chinese (automatic)"),
            ),
        ] {
            self.control.addItemWithTitle(&NSString::from_str(title));
            values.push(value);
        }
        for source in sources {
            let title = if sources
                .iter()
                .filter(|other| other.name == source.name)
                .count()
                > 1
            {
                format!("{} ({})", source.name, source.id)
            } else {
                source.name.clone()
            };
            self.control.addItemWithTitle(&NSString::from_str(&title));
            values.push(SourceRule::Source(source.clone()));
        }
        let index = values
            .iter()
            .position(|value| same_source(value, &selected))
            .unwrap_or_else(|| {
                if let SourceRule::Source(source) = &selected {
                    self.control.addItemWithTitle(&NSString::from_str(&trf!(
                        "{}（不可用）",
                        "{} (unavailable)",
                        source.name
                    )));
                    values.push(selected);
                    values.len() - 1
                } else {
                    0
                }
            });
        self.control.selectItemAtIndex(index as isize);
        self.values.replace(values);
    }
    fn read(&self) -> SourceRule {
        self.values
            .borrow()
            .get(self.control.indexOfSelectedItem() as usize)
            .cloned()
            .unwrap_or_default()
    }
}

fn same_source(a: &SourceRule, b: &SourceRule) -> bool {
    match (a, b) {
        (SourceRule::Source(a), SourceRule::Source(b)) => a.id == b.id,
        _ => a == b,
    }
}

pub(super) struct InputRulesPage {
    enabled: Retained<NSButton>,
    source: SourcePicker,
    restore: Retained<NSPopUpButton>,
    scroll: Retained<NSScrollView>,
    document: Retained<NSView>,
    empty: Retained<NSTextField>,
    rows: RefCell<Vec<Rc<RuleRow>>>,
    target: Weak<AnyObject>,
}

impl InputRulesPage {
    pub(super) fn new(host: &NSView, target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let group = settings_group(host, tr!("全局规则", "Global rule"), 570.0, 172.0, mtm);
        let enabled = checkbox(
            tr!(
                "为其他应用启用自动切换",
                "Enable automatic switching for other apps"
            ),
            mtm,
        );
        enabled.setFrame(rect(20.0, 130.0, 700.0, 28.0));
        set_action(&enabled, target, sel!(settingsChanged:));
        group.addSubview(&enabled);
        row_divider(&group, 120.0, mtm);
        group.addSubview(&label(
            tr!("默认输入法", "Default input source"),
            14.0,
            rect(20.0, 79.0, 270.0, 26.0),
            mtm,
        ));
        let source = SourcePicker::new(rect(310.0, 78.0, 410.0, 28.0), false, target, mtm);
        group.addSubview(&source.control);
        group.addSubview(&label(
            tr!("回到应用时", "When returning to an app"),
            14.0,
            rect(20.0, 28.0, 270.0, 26.0),
            mtm,
        ));
        let restore = popup(
            &[
                tr!("使用默认输入法", "Use default input source"),
                tr!("恢复该应用上次使用", "Restore last used in that app"),
            ],
            rect(310.0, 27.0, 410.0, 28.0),
            mtm,
        );
        set_action(&restore, target, sel!(settingsChanged:));
        group.addSubview(&restore);
        host.addSubview(&label(
            tr!("应用规则", "App rules"),
            14.0,
            rect(8.0, 329.0, 300.0, 27.0),
            mtm,
        ));
        host.addSubview(&button(
            tr!("＋ 添加应用…", "＋ Add App…"),
            target,
            sel!(addInputRule:),
            rect(554.0, 328.0, 182.0, 30.0),
            mtm,
        ));
        let scroll =
            NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(0.0, 53.0, 740.0, 265.0));
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setScrollerStyle(NSScrollerStyle::Overlay);
        scroll.setDrawsBackground(false);
        let document = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 740.0, 265.0));
        let empty = hint(
            tr!(
                "添加应用，为它覆盖全局输入法或恢复策略。",
                "Add an app to override its input source or restore strategy."
            ),
            rect(20.0, 209.0, 690.0, 40.0),
            mtm,
        );
        document.addSubview(&empty);
        scroll.setDocumentView(Some(&document));
        host.addSubview(&scroll);
        host.addSubview(&hint(tr!("应用规则优先；Winlane 规则始终生效。输入过程中仍可手动切换。", "App rules take priority; the Winlane rule is always active. You can still switch sources manually."), rect(8.0, 3.0, 726.0, 42.0), mtm));
        Self {
            enabled,
            source,
            restore,
            scroll,
            document,
            empty,
            rows: RefCell::default(),
            target: Weak::new(target),
        }
    }

    fn sources(&self) -> Vec<InputSource> {
        Source::enabled(self.enabled.mtm())
            .into_iter()
            .map(|source| InputSource {
                id: source.id,
                name: source.name,
            })
            .collect()
    }
    pub(super) fn fill(&self, config: &Config) {
        let settings = &config.input_rules;
        self.enabled.setState(if settings.enabled {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        let sources = self.sources();
        self.source.fill(settings.default_source.clone(), &sources);
        self.restore
            .selectItemAtIndex(isize::from(settings.restore == RestoreStrategy::LastUsed));
        for row in self.rows.borrow_mut().drain(..) {
            row.view.removeFromSuperview();
        }
        for rule in &settings.apps {
            let row = self.append_row();
            Self::set_application(&row, rule.application.clone());
            row.source.fill(rule.source.clone(), &sources);
            row.restore.selectItemAtIndex(match rule.restore {
                None => 0,
                Some(RestoreStrategy::Default) => 1,
                Some(RestoreStrategy::LastUsed) => 2,
            });
        }
        self.layout();
        self.document.scrollRectToVisible(rect(
            0.0,
            self.document.bounds().size.height - 1.0,
            740.0,
            1.0,
        ));
    }
    pub(super) fn refresh_sources(&self) {
        let sources = self.sources();
        self.source.fill(self.source.read(), &sources);
        for row in self.rows.borrow().iter() {
            row.source.fill(row.source.read(), &sources);
        }
    }
    pub(super) fn read(&self, config: &mut Config) {
        let settings = &mut config.input_rules;
        settings.enabled = self.enabled.state() == NSControlStateValueOn;
        settings.default_source = self.source.read();
        settings.restore = if self.restore.indexOfSelectedItem() == 1 {
            RestoreStrategy::LastUsed
        } else {
            RestoreStrategy::Default
        };
        settings.apps = self
            .rows
            .borrow()
            .iter()
            .filter_map(|row| {
                Some(AppRule {
                    application: row.application.borrow().clone()?,
                    source: row.source.read(),
                    restore: match row.restore.indexOfSelectedItem() {
                        1 => Some(RestoreStrategy::Default),
                        2 => Some(RestoreStrategy::LastUsed),
                        _ => None,
                    },
                })
            })
            .collect();
    }
    fn append_row(&self) -> Rc<RuleRow> {
        let mtm = self.enabled.mtm();
        let target = self
            .target
            .load()
            .expect("settings delegate lives for the app lifetime");
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 740.0, 130.0));
        let choose = button(
            tr!("选择应用…", "Choose App…"),
            &target,
            sel!(chooseInputRuleApp:),
            rect(10.0, 89.0, 588.0, 30.0),
            mtm,
        );
        let remove = button(
            tr!("移除", "Remove"),
            &target,
            sel!(removeInputRule:),
            rect(615.0, 89.0, 112.0, 30.0),
            mtm,
        );
        view.addSubview(&choose);
        view.addSubview(&remove);
        let source = SourcePicker::new(rect(10.0, 22.0, 350.0, 28.0), true, &target, mtm);
        source.fill(SourceRule::Global, &self.sources());
        view.addSubview(&hint(
            tr!("默认输入法", "Default input source"),
            rect(14.0, 53.0, 340.0, 23.0),
            mtm,
        ));
        view.addSubview(&source.control);
        let restore = popup(
            &[
                tr!("使用全局设置", "Use global setting"),
                tr!("使用默认输入法", "Use default input source"),
                tr!("恢复该应用上次使用", "Restore last used in that app"),
            ],
            rect(378.0, 22.0, 349.0, 28.0),
            mtm,
        );
        set_action(&restore, &target, sel!(settingsChanged:));
        view.addSubview(&hint(
            tr!("回到应用时", "When returning to an app"),
            rect(382.0, 53.0, 340.0, 23.0),
            mtm,
        ));
        view.addSubview(&restore);
        row_divider(&view, 4.0, mtm);
        let row = Rc::new(RuleRow {
            view,
            choose,
            remove,
            source,
            restore,
            application: RefCell::default(),
        });
        self.document.addSubview(&row.view);
        self.rows.borrow_mut().push(row.clone());
        row
    }
    fn set_application(row: &RuleRow, application: ApplicationTarget) {
        row.choose.setTitle(&NSString::from_str(&application.name));
        row.choose
            .setToolTip(Some(&NSString::from_str(&application.bundle_id)));
        let builtin = application.bundle_id == WINLANE_ID;
        row.choose.setEnabled(!builtin);
        row.remove.setEnabled(!builtin);
        row.application.replace(Some(application));
    }
    fn layout(&self) {
        let rows = self.rows.borrow();
        let height = (rows.len() as f64 * 130.0).max(self.scroll.contentSize().height);
        self.document.setFrameSize(NSSize::new(740.0, height));
        self.empty.setHidden(!rows.is_empty());
        for (index, row) in rows.iter().enumerate() {
            row.view
                .setFrameOrigin(NSPoint::new(0.0, height - (index + 1) as f64 * 130.0));
            row.choose.setTag(index as isize);
            row.remove.setTag(index as isize);
        }
    }
    pub(super) fn add(&self, window: &NSWindow, message: &Retained<NSTextField>) {
        if self.rows.borrow().len() >= 64 {
            return;
        }
        let row = self.append_row();
        self.layout();
        row.view.scrollRectToVisible(row.view.bounds());
        self.choose(self.rows.borrow().len() - 1, window, message);
    }
    pub(super) fn remove(&self, index: usize) {
        let removable = self
            .rows
            .borrow()
            .get(index)
            .is_some_and(|row| row.remove.isEnabled());
        if removable {
            self.rows
                .borrow_mut()
                .remove(index)
                .view
                .removeFromSuperview();
        }
        self.layout();
    }
    pub(super) fn choose(&self, index: usize, window: &NSWindow, message: &Retained<NSTextField>) {
        let Some(row) = self.rows.borrow().get(index).cloned() else {
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
        panel.setPrompt(Some(&NSString::from_str(tr!("选择应用", "Choose App"))));
        let selected = panel.clone();
        let message = message.clone();
        let completion = RcBlock::new(move |response| {
            if response != NSModalResponseOK || row.view.window().is_none() {
                return;
            }
            if let Some(url) = selected.URL() {
                match target_at_url(&url) {
                    Ok(application) if application.bundle_id == WINLANE_ID => {
                        message.setStringValue(&NSString::from_str(tr!(
                            "请编辑列表中已有的 Winlane 规则。",
                            "Edit the existing Winlane rule in the list."
                        )));
                        message.setTextColor(Some(&NSColor::systemRedColor()));
                    }
                    Ok(application) => {
                        Self::set_application(&row, application);
                        // SAFETY: The control has the live settings delegate as its action target.
                        unsafe {
                            row.source.control.sendAction_to(
                                row.source.control.action(),
                                row.source.control.target().as_deref(),
                            );
                        }
                    }
                    Err(error) => {
                        message.setStringValue(&NSString::from_str(&error));
                        message.setTextColor(Some(&NSColor::systemRedColor()));
                    }
                }
            }
        });
        panel.beginSheetModalForWindow_completionHandler(window, &completion);
    }
}

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../../tests/native/settings_input_rules.rs"]
pub(crate) mod tests;
