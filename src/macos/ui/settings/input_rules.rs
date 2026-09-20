use super::*;
use crate::macos::platform::{applications::target_at_url, input_source::Source};
use crate::macos::ui::rule_list::RuleListButton;
use block2::RcBlock;
use objc2_foundation::{NSArray, NSURL, ns_string};
use std::cell::Cell;
use std::rc::Rc;
use winlane::core::config::ApplicationTarget;
use winlane::features::input_rules::{
    AppRule, InputSource, RestoreStrategy, SourceRule, WINLANE_ID,
};

struct RuleRow {
    navigation: Retained<RuleListButton>,
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
    detail: Retained<NSView>,
    global_view: Retained<NSView>,
    global_navigation: Retained<RuleListButton>,
    selected: Cell<usize>,
    rows: RefCell<Vec<Rc<RuleRow>>>,
    target: Weak<AnyObject>,
}

impl InputRulesPage {
    pub(super) fn new(host: &NSView, target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let enabled = checkbox(
            tr!(
                "为其他应用启用自动切换",
                "Enable automatic switching for other apps"
            ),
            mtm,
        );
        enabled.setFrame(rect(4.0, 533.0, 730.0, 28.0));
        set_action(&enabled, target, sel!(settingsChanged:));
        host.addSubview(&enabled);
        let scroll =
            NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(0.0, 50.0, 250.0, 462.0));
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setScrollerStyle(NSScrollerStyle::Overlay);
        scroll.setDrawsBackground(false);
        let document = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 250.0, 462.0));
        scroll.setDocumentView(Some(&document));
        host.addSubview(&scroll);
        host.addSubview(&button(
            tr!("＋ 添加应用…", "＋ Add App…"),
            target,
            sel!(addInputRule:),
            rect(0.0, 10.0, 250.0, 30.0),
            mtm,
        ));
        let divider = NSBox::initWithFrame(NSBox::alloc(mtm), rect(262.0, 10.0, 1.0, 502.0));
        divider.setBoxType(NSBoxType::Separator);
        host.addSubview(&divider);
        let detail = NSView::initWithFrame(NSView::alloc(mtm), rect(274.0, 50.0, 466.0, 462.0));
        host.addSubview(&detail);
        let global_view = NSView::initWithFrame(NSView::alloc(mtm), detail.bounds());
        global_view.addSubview(&label(
            tr!("全局规则", "Global rule"),
            20.0,
            rect(20.0, 407.0, 426.0, 32.0),
            mtm,
        ));
        global_view.addSubview(&hint(
            tr!(
                "未单独设置的应用使用此规则。",
                "Used by apps without their own rule."
            ),
            rect(20.0, 373.0, 426.0, 25.0),
            mtm,
        ));
        global_view.addSubview(&label(
            tr!("默认输入法", "Default input source"),
            14.0,
            rect(20.0, 329.0, 426.0, 24.0),
            mtm,
        ));
        let source = SourcePicker::new(rect(20.0, 293.0, 426.0, 28.0), false, target, mtm);
        global_view.addSubview(&source.control);
        global_view.addSubview(&label(
            tr!("回到应用时", "When returning to an app"),
            14.0,
            rect(20.0, 233.0, 426.0, 24.0),
            mtm,
        ));
        let restore = popup(
            &[
                tr!("使用默认输入法", "Use default input source"),
                tr!("恢复该应用上次使用", "Restore last used in that app"),
            ],
            rect(20.0, 197.0, 426.0, 28.0),
            mtm,
        );
        set_action(&restore, target, sel!(settingsChanged:));
        global_view.addSubview(&restore);
        global_view.addSubview(&hint(
            tr!(
                "应用规则优先；Winlane 规则始终生效。",
                "App rules take priority; the Winlane rule is always active."
            ),
            rect(20.0, 118.0, 426.0, 48.0),
            mtm,
        ));
        detail.addSubview(&global_view);
        let global_navigation = RuleListButton::new(target, sel!(selectInputRule:), mtm);
        global_navigation.set_label(tr!("全局规则", "Global rule"));
        global_navigation.set_symbol("globe");
        global_navigation.setTag(0);
        document.addSubview(&global_navigation);
        Self {
            enabled,
            source,
            restore,
            scroll,
            document,
            detail,
            global_view,
            global_navigation,
            selected: Cell::new(0),
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
            row.navigation.removeFromSuperview();
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
        self.selected.set(0);
        self.layout();
        self.document.scrollRectToVisible(rect(
            0.0,
            self.document.bounds().size.height - 1.0,
            250.0,
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
        let view = NSView::initWithFrame(NSView::alloc(mtm), self.detail.bounds());
        let navigation = RuleListButton::new(&target, sel!(selectInputRule:), mtm);
        navigation.set_label(tr!("新应用规则", "New app rule"));
        navigation.set_symbol("app.dashed");
        let choose = button(
            tr!("选择应用…", "Choose App…"),
            &target,
            sel!(chooseInputRuleApp:),
            rect(20.0, 403.0, 426.0, 32.0),
            mtm,
        );
        let remove = button(
            tr!("移除规则", "Remove Rule"),
            &target,
            sel!(removeInputRule:),
            rect(306.0, 12.0, 140.0, 30.0),
            mtm,
        );
        view.addSubview(&choose);
        view.addSubview(&remove);
        view.addSubview(&label(
            tr!("默认输入法", "Default input source"),
            14.0,
            rect(20.0, 329.0, 426.0, 24.0),
            mtm,
        ));
        let source = SourcePicker::new(rect(20.0, 293.0, 426.0, 28.0), true, &target, mtm);
        source.fill(SourceRule::Global, &self.sources());
        view.addSubview(&source.control);
        view.addSubview(&label(
            tr!("回到应用时", "When returning to an app"),
            14.0,
            rect(20.0, 233.0, 426.0, 24.0),
            mtm,
        ));
        let restore = popup(
            &[
                tr!("使用全局设置", "Use global setting"),
                tr!("使用默认输入法", "Use default input source"),
                tr!("恢复该应用上次使用", "Restore last used in that app"),
            ],
            rect(20.0, 197.0, 426.0, 28.0),
            mtm,
        );
        set_action(&restore, &target, sel!(settingsChanged:));
        view.addSubview(&restore);
        let row = Rc::new(RuleRow {
            view,
            navigation,
            choose,
            remove,
            source,
            restore,
            application: RefCell::default(),
        });
        row.view.setHidden(true);
        self.detail.addSubview(&row.view);
        self.document.addSubview(&row.navigation);
        self.rows.borrow_mut().push(row.clone());
        row
    }
    fn set_application(row: &RuleRow, application: ApplicationTarget) {
        row.choose.setTitle(&NSString::from_str(&application.name));
        row.choose
            .setToolTip(Some(&NSString::from_str(&application.bundle_id)));
        row.navigation.set_label(&application.name);
        row.navigation.set_application_icon(&application.path);
        let builtin = application.bundle_id == WINLANE_ID;
        row.choose.setEnabled(!builtin);
        row.remove.setEnabled(!builtin);
        row.application.replace(Some(application));
    }
    fn layout(&self) {
        let rows = self.rows.borrow();
        let height = ((rows.len() + 1) as f64 * 42.0).max(self.scroll.contentSize().height);
        self.document.setFrameSize(NSSize::new(250.0, height));
        self.global_navigation
            .setFrame(rect(4.0, height - 42.0, 242.0, 40.0));
        self.global_navigation
            .set_selected(self.selected.get() == 0);
        self.global_view.setHidden(self.selected.get() != 0);
        for (index, row) in rows.iter().enumerate() {
            row.navigation
                .setFrame(rect(4.0, height - (index + 2) as f64 * 42.0, 242.0, 40.0));
            row.navigation.setTag((index + 1) as isize);
            row.navigation
                .set_selected(self.selected.get() == index + 1);
            row.view.setHidden(self.selected.get() != index + 1);
            row.choose.setTag(index as isize);
            row.remove.setTag(index as isize);
        }
    }
    pub(super) fn select(&self, index: usize) {
        if index > self.rows.borrow().len() {
            return;
        }
        if let Some(window) = self.detail.window() {
            window.makeFirstResponder(None);
        }
        self.selected.set(index);
        self.layout();
        if index == 0 {
            self.global_navigation
                .scrollRectToVisible(self.global_navigation.bounds());
        } else if let Some(row) = self.rows.borrow().get(index - 1) {
            row.navigation.scrollRectToVisible(row.navigation.bounds());
        }
    }
    pub(super) fn add(&self, window: &NSWindow, message: &Retained<NSTextField>) {
        if self.rows.borrow().len() >= 64 {
            return;
        }
        self.append_row();
        let count = self.rows.borrow().len();
        self.select(count);
        self.choose(count - 1, window, message);
    }
    pub(super) fn remove(&self, index: usize) {
        let removable = self
            .rows
            .borrow()
            .get(index)
            .is_some_and(|row| row.remove.isEnabled());
        if removable {
            let row = self.rows.borrow_mut().remove(index);
            row.view.removeFromSuperview();
            row.navigation.removeFromSuperview();
            let selected = self.selected.get();
            self.selected.set(if selected > index + 1 {
                selected - 1
            } else {
                selected.min(self.rows.borrow().len())
            });
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
