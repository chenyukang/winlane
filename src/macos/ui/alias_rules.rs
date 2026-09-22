use super::rule_list::RuleListButton;
use crate::macos::ui::controls::{button, hint, label, popup, rect, set_action};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadOnly, sel};
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSArray, NSSize, NSString, NSURL, ns_string};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use winlane::core::commands::{COMMANDS, CommandId};
use winlane::core::config::{AliasRule, ApplicationTarget, Config};
use winlane::tr;

struct Row {
    navigation: Retained<RuleListButton>,
    view: Retained<NSView>,
    kind: Retained<NSPopUpButton>,
    choose: Retained<NSButton>,
    remove: Retained<NSButton>,
    alias: Retained<NSTextField>,
    title: Retained<NSTextField>,
    title_label: Retained<NSTextField>,
    title_hint: Retained<NSTextField>,
    command: Retained<NSPopUpButton>,
    command_label: Retained<NSTextField>,
    application: RefCell<Option<ApplicationTarget>>,
}

impl Row {
    fn is_command(&self) -> bool {
        self.kind.indexOfSelectedItem() == 1
    }
    fn selected_command(&self) -> CommandId {
        let index = self.command.indexOfSelectedItem().max(0) as usize;
        COMMANDS
            .get(index)
            .map_or(COMMANDS[0].id, |command| command.id)
    }
    fn set_kind_command(&self, command: bool) {
        self.kind.selectItemAtIndex(if command { 1 } else { 0 });
        self.apply_kind_visibility();
    }
    fn select_command(&self, id: CommandId) {
        if let Some(index) = COMMANDS.iter().position(|command| command.id == id) {
            self.command.selectItemAtIndex(index as isize);
            self.apply_kind_visibility();
        }
    }
    // Show the app chooser and title filter for app rules, the command picker for command rules.
    fn apply_kind_visibility(&self) {
        let command = self.is_command();
        self.choose.setHidden(command);
        self.title.setHidden(command);
        self.title_label.setHidden(command);
        self.title_hint.setHidden(command);
        self.command.setHidden(!command);
        self.command_label.setHidden(!command);
        if command {
            self.navigation
                .set_symbol(self.selected_command().definition().symbol);
        } else if let Some(app) = self.application.borrow().as_ref() {
            self.navigation.set_application_icon(&app.path);
        } else {
            self.navigation.set_symbol("app.dashed");
        }
    }
    fn set_application(&self, app: ApplicationTarget) {
        self.choose.setTitle(&NSString::from_str(&app.name));
        self.choose.setToolTip(Some(&NSString::from_str(&app.path)));
        self.navigation.set_application_icon(&app.path);
        self.application.replace(Some(app));
        self.refresh_label();
    }
    fn refresh_label(&self) {
        let alias = self.alias.stringValue().to_string();
        let name = if self.is_command() {
            self.selected_command().definition().title().to_owned()
        } else {
            self.application
                .borrow()
                .as_ref()
                .map_or(tr!("新规则", "New rule").to_owned(), |app| {
                    app.name.clone()
                })
        };
        let label = if alias.trim().is_empty() {
            name.clone()
        } else {
            format!("{} · {name}", alias.trim())
        };
        self.navigation.set_label(&label);
        let title = self.title.stringValue().to_string();
        if !self.is_command() && !title.trim().is_empty() {
            self.navigation
                .setToolTip(Some(&NSString::from_str(&format!(
                    "{label} — {}",
                    title.trim()
                ))));
        }
    }
    fn notify_changed(&self) {
        // SAFETY: The controls target the application delegate for its lifetime.
        unsafe {
            self.alias
                .sendAction_to(self.alias.action(), self.alias.target().as_deref());
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AliasRulesDraft {
    rows: Vec<RowDraft>,
    selected: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RowDraft {
    application: Option<ApplicationTarget>,
    command: Option<CommandId>,
    alias: String,
    title: String,
}

pub struct AliasRulesEditor {
    root: Retained<NSView>,
    document: Retained<NSView>,
    scroll: Retained<NSScrollView>,
    rows: RefCell<Vec<Rc<Row>>>,
    message: Retained<NSTextField>,
    detail: Retained<NSView>,
    empty: Retained<NSTextField>,
    selected: Cell<Option<usize>>,
}

impl AliasRulesEditor {
    pub fn new(target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let root = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 740.0, 574.0));
        root.addSubview(&button(
            tr!("＋ 添加规则", "＋ Add Rule"),
            target,
            sel!(addAliasRule:),
            rect(0.0, 10.0, 250.0, 30.0),
            mtm,
        ));
        let scroll =
            NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(0.0, 50.0, 250.0, 514.0));
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setScrollerStyle(NSScrollerStyle::Overlay);
        scroll.setDrawsBackground(false);
        let document = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 250.0, 514.0));
        scroll.setDocumentView(Some(&document));
        root.addSubview(&scroll);
        let divider = NSBox::initWithFrame(NSBox::alloc(mtm), rect(262.0, 10.0, 1.0, 554.0));
        divider.setBoxType(NSBoxType::Separator);
        root.addSubview(&divider);
        let detail = NSView::initWithFrame(NSView::alloc(mtm), rect(274.0, 50.0, 466.0, 514.0));
        root.addSubview(&detail);
        let empty = hint(
            tr!(
                "添加规则，为应用、项目窗口或命令设置专用字母。",
                "Add a rule to assign an alias to an app, project window, or command."
            ),
            rect(20.0, 406.0, 426.0, 60.0),
            mtm,
        );
        detail.addSubview(&empty);
        let message = hint(
            tr!("完整规则自动保存。", "Complete rules save automatically."),
            rect(294.0, 4.0, 426.0, 42.0),
            mtm,
        );
        root.addSubview(&message);
        Self {
            root,
            document,
            scroll,
            detail,
            empty,
            selected: Cell::new(None),
            rows: RefCell::default(),
            message,
        }
    }

    pub fn view(&self) -> &NSView {
        &self.root
    }

    fn finish_editing(&self) {
        if let Some(window) = self.root.window() {
            window.makeFirstResponder(None);
        }
    }

    fn add_row(&self, target: &AnyObject, mtm: MainThreadMarker) -> Rc<Row> {
        let view = NSView::initWithFrame(NSView::alloc(mtm), self.detail.bounds());
        let navigation = RuleListButton::new(target, sel!(selectAliasRule:), mtm);
        navigation.set_symbol("app.dashed");
        let kind = popup(&[], rect(20.0, 476.0, 200.0, 28.0), mtm);
        kind.addItemWithTitle(&NSString::from_str(tr!("应用", "Application")));
        kind.addItemWithTitle(&NSString::from_str(tr!("命令", "Command")));
        set_action(&kind, target, sel!(aliasRuleTargetChanged:));
        kind.setAccessibilityLabel(Some(&NSString::from_str(tr!("绑定类型", "Target type"))));
        view.addSubview(&kind);
        view.addSubview(&hint(
            tr!("绑定应用或命令", "Bind an app or command"),
            rect(230.0, 478.0, 216.0, 24.0),
            mtm,
        ));
        let choose = button(
            tr!("选择应用…", "Choose App…"),
            target,
            sel!(chooseAliasApp:),
            rect(20.0, 356.0, 426.0, 30.0),
            mtm,
        );
        let remove = button(
            tr!("移除规则", "Remove Rule"),
            target,
            sel!(removeAliasRule:),
            rect(314.0, 12.0, 132.0, 30.0),
            mtm,
        );
        view.addSubview(&choose);
        view.addSubview(&remove);
        let alias =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(20.0, 402.0, 90.0, 28.0));
        alias.setPlaceholderString(Some(ns_string!("ck")));
        alias.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "Alias 字母",
            "Alias letters"
        ))));
        let title =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(20.0, 276.0, 426.0, 28.0));
        title.setPlaceholderString(Some(&NSString::from_str(tr!(
            "标题包含（可选），例如 ckb",
            "Title contains (optional), e.g. ckb"
        ))));
        title.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "标题关键词",
            "Title keywords"
        ))));
        for field in [&alias, &title] {
            field.cell().unwrap().setSendsActionOnEndEditing(true);
            unsafe {
                field.setTarget(Some(target));
                field.setAction(Some(sel!(aliasRulesChanged:)));
            }
            view.addSubview(field);
        }
        view.addSubview(&label(
            tr!("Alias 字母", "Alias letters"),
            14.0,
            rect(20.0, 438.0, 426.0, 24.0),
            mtm,
        ));
        view.addSubview(&hint(
            tr!("1–2 个小写字母", "1–2 lowercase letters"),
            rect(122.0, 402.0, 324.0, 24.0),
            mtm,
        ));
        let title_label = label(
            tr!("窗口标题包含", "Window title contains"),
            14.0,
            rect(20.0, 312.0, 426.0, 24.0),
            mtm,
        );
        view.addSubview(&title_label);
        let title_hint = hint(
            tr!(
                "可选。留空则匹配此应用的所有窗口。",
                "Optional. Leave empty to match all windows in this app."
            ),
            rect(20.0, 228.0, 426.0, 40.0),
            mtm,
        );
        view.addSubview(&title_hint);
        let command_label = label(
            tr!("命令", "Command"),
            14.0,
            rect(20.0, 356.0, 426.0, 24.0),
            mtm,
        );
        view.addSubview(&command_label);
        let command = popup(&[], rect(20.0, 320.0, 426.0, 28.0), mtm);
        for definition in COMMANDS {
            command.addItemWithTitle(&NSString::from_str(definition.title()));
        }
        set_action(&command, target, sel!(aliasRuleTargetChanged:));
        command.setAccessibilityLabel(Some(&NSString::from_str(tr!("命令", "Command"))));
        view.addSubview(&command);
        let row = Rc::new(Row {
            navigation,
            view,
            kind,
            choose,
            remove,
            alias,
            title,
            title_label,
            title_hint,
            command,
            command_label,
            application: RefCell::default(),
        });
        row.apply_kind_visibility();
        row.refresh_label();
        row.view.setHidden(true);
        self.detail.addSubview(&row.view);
        self.document.addSubview(&row.navigation);
        self.rows.borrow_mut().push(row.clone());
        row
    }

    pub fn fill(&self, rules: &[AliasRule], target: &AnyObject, mtm: MainThreadMarker) {
        for row in self.rows.take() {
            row.view.removeFromSuperview();
            row.navigation.removeFromSuperview();
        }
        for rule in rules {
            let row = self.add_row(target, mtm);
            if let Some(application) = &rule.application {
                row.set_application(application.clone());
            }
            if let Some(command) = rule.command {
                row.set_kind_command(true);
                row.select_command(command);
            }
            row.alias.setStringValue(&NSString::from_str(&rule.alias));
            row.title
                .setStringValue(&NSString::from_str(&rule.title_contains));
        }
        self.selected.set((!rules.is_empty()).then_some(0));
        self.layout();
    }

    pub fn draft(&self) -> AliasRulesDraft {
        AliasRulesDraft {
            rows: self
                .rows
                .borrow()
                .iter()
                .map(|row| RowDraft {
                    application: row.application.borrow().clone(),
                    command: row.is_command().then(|| row.selected_command()),
                    alias: row.alias.stringValue().to_string(),
                    title: row.title.stringValue().to_string(),
                })
                .collect(),
            selected: self.selected.get(),
        }
    }

    pub fn restore_draft(&self, draft: AliasRulesDraft, target: &AnyObject, mtm: MainThreadMarker) {
        self.fill(&[], target, mtm);
        for old in draft.rows {
            let row = self.add_row(target, mtm);
            if let Some(app) = old.application {
                row.set_application(app);
            }
            if let Some(command) = old.command {
                row.set_kind_command(true);
                row.select_command(command);
            }
            row.alias.setStringValue(&NSString::from_str(&old.alias));
            row.title.setStringValue(&NSString::from_str(&old.title));
        }
        self.selected.set(draft.selected);
        self.layout();
    }

    pub fn add(&self, target: &AnyObject, mtm: MainThreadMarker) {
        if self.rows.borrow().len() >= 64 {
            self.report(
                tr!("最多设置 64 条规则。", "You can configure up to 64 rules."),
                true,
            );
            return;
        }
        self.finish_editing();
        self.add_row(target, mtm);
        let index = self.rows.borrow().len() - 1;
        self.select(index);
    }

    pub fn remove(&self, index: usize) {
        self.finish_editing();
        if index < self.rows.borrow().len() {
            let row = self.rows.borrow_mut().remove(index);
            row.view.removeFromSuperview();
            row.navigation.removeFromSuperview();
            let count = self.rows.borrow().len();
            let selected = self.selected.get().unwrap_or(0);
            self.selected.set(if count == 0 {
                None
            } else {
                Some(if selected > index {
                    selected - 1
                } else {
                    selected.min(count - 1)
                })
            });
            self.layout();
        }
    }

    pub fn select(&self, index: usize) {
        if index >= self.rows.borrow().len() {
            return;
        }
        // Finish editing before hiding the field, so switching rules also autosaves it.
        self.finish_editing();
        self.selected.set(Some(index));
        self.layout();
        let row = &self.rows.borrow()[index];
        row.navigation.scrollRectToVisible(row.navigation.bounds());
    }

    // Refresh a row's controls after its target type or command changed.
    pub fn update_target(&self, index: usize) {
        if let Some(row) = self.rows.borrow().get(index).cloned() {
            row.apply_kind_visibility();
            row.refresh_label();
        }
    }

    fn layout(&self) {
        let rows = self.rows.borrow();
        let height = (rows.len() as f64 * 42.0).max(self.scroll.contentSize().height);
        self.document.setFrameSize(NSSize::new(250.0, height));
        self.empty.setHidden(!rows.is_empty());
        for (index, row) in rows.iter().enumerate() {
            row.navigation
                .setFrame(rect(4.0, height - (index + 1) as f64 * 42.0, 242.0, 40.0));
            row.navigation.setTag(index as isize);
            row.navigation
                .set_selected(self.selected.get() == Some(index));
            row.refresh_label();
            row.view.setHidden(self.selected.get() != Some(index));
            row.kind.setTag(index as isize);
            row.command.setTag(index as isize);
            row.choose.setTag(index as isize);
            row.remove.setTag(index as isize);
        }
    }

    pub fn choose(&self, index: usize, mtm: MainThreadMarker) {
        let Some(row) = self.rows.borrow().get(index).cloned() else {
            return;
        };
        let Some(window) = self.root.window() else {
            return;
        };
        let panel = NSOpenPanel::openPanel(mtm);
        panel.setCanChooseFiles(true);
        panel.setCanChooseDirectories(false);
        panel.setAllowsMultipleSelection(false);
        panel.setTreatsFilePackagesAsDirectories(false);
        #[allow(deprecated)]
        panel.setAllowedFileTypes(Some(&NSArray::from_slice(&[ns_string!("app")])));
        panel.setDirectoryURL(Some(&NSURL::fileURLWithPath(ns_string!("/Applications"))));
        let selected = panel.clone();
        let message = self.message.clone();
        let completion = RcBlock::new(move |response| {
            if response == NSModalResponseOK
                && row.view.window().is_some()
                && let Some(url) = selected.URL()
            {
                match crate::macos::platform::applications::target_at_url(&url) {
                    Ok(app) => {
                        row.set_application(app);
                        row.notify_changed();
                    }
                    Err(error) => {
                        message.setStringValue(&NSString::from_str(&error));
                        message.setTextColor(Some(&NSColor::systemRedColor()));
                    }
                }
            }
        });
        panel.beginSheetModalForWindow_completionHandler(&window, &completion);
    }

    pub fn candidate(&self) -> Result<Vec<AliasRule>, String> {
        for row in self.rows.borrow().iter() {
            row.refresh_label();
        }
        let rules: Vec<_> = self
            .rows
            .borrow()
            .iter()
            .filter_map(|row| {
                let alias = row.alias.stringValue().to_string().trim().to_string();
                if alias.is_empty() {
                    return None;
                }
                if row.is_command() {
                    return Some(AliasRule {
                        alias,
                        application: None,
                        command: Some(row.selected_command()),
                        title_contains: String::new(),
                    });
                }
                let application = row.application.borrow().clone()?;
                Some(AliasRule {
                    alias,
                    application: Some(application),
                    command: None,
                    title_contains: row.title.stringValue().to_string().trim().into(),
                })
            })
            .collect();
        Config {
            alias_rules: rules.clone(),
            ..Config::default()
        }
        .validate()?;
        Ok(rules)
    }

    pub fn report(&self, text: &str, error: bool) {
        self.message.setStringValue(&NSString::from_str(text));
        let color = if error {
            NSColor::systemRedColor()
        } else {
            NSColor::secondaryLabelColor()
        };
        self.message.setTextColor(Some(&color));
    }
}

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../tests/native/alias_rules.rs"]
pub(crate) mod tests;
