use crate::settings::{button, hint, label, preferences_window, rect};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadOnly, sel};
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSArray, NSPoint, NSSize, NSString, NSURL, ns_string};
use std::cell::RefCell;
use std::rc::Rc;
use winlane::config::{AliasRule, ApplicationTarget, Config};
use winlane::tr;

struct Row {
    view: Retained<NSView>,
    choose: Retained<NSButton>,
    remove: Retained<NSButton>,
    alias: Retained<NSTextField>,
    title: Retained<NSTextField>,
    application: RefCell<Option<ApplicationTarget>>,
}

impl Row {
    fn set_application(&self, app: ApplicationTarget) {
        self.choose.setTitle(&NSString::from_str(&app.name));
        self.choose.setToolTip(Some(&NSString::from_str(&app.path)));
        self.application.replace(Some(app));
    }
    fn notify_changed(&self) {
        // SAFETY: The controls target the application delegate for its lifetime.
        unsafe {
            self.alias
                .sendAction_to(self.alias.action(), self.alias.target().as_deref());
        }
    }
}

pub struct AliasRulesWindow {
    pub window: Retained<NSWindow>,
    document: Retained<NSView>,
    scroll: Retained<NSScrollView>,
    rows: RefCell<Vec<Rc<Row>>>,
    message: Retained<NSTextField>,
}

impl AliasRulesWindow {
    pub fn new(target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let window = preferences_window(rect(0.0, 0.0, 700.0, 570.0), mtm);
        window.setTitle(&NSString::from_str(tr!(
            "Winlane · Alias 规则",
            "Winlane · Alias Rules"
        )));
        let root = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 700.0, 570.0));
        window.setContentView(Some(&root));
        root.addSubview(&label(
            tr!("自定义 Alias", "Custom aliases"),
            23.0,
            rect(30.0, 508.0, 480.0, 32.0),
            mtm,
        ));
        root.addSubview(&hint(tr!("选择应用，填写 1–2 个小写字母。标题关键词可选，用于区分项目窗口。", "Choose an app and 1–2 lowercase letters. Optional title keywords target a project window."), rect(30.0, 456.0, 640.0, 44.0), mtm));
        root.addSubview(&button(
            tr!("＋ 添加", "＋ Add"),
            target,
            sel!(addAliasRule:),
            rect(555.0, 508.0, 115.0, 30.0),
            mtm,
        ));
        let scroll =
            NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(20.0, 104.0, 660.0, 338.0));
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setDrawsBackground(false);
        let document = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 660.0, 338.0));
        scroll.setDocumentView(Some(&document));
        root.addSubview(&scroll);
        let message = NSTextField::wrappingLabelWithString(
            &NSString::from_str(tr!(
                "完整规则自动保存。自定义字母不会被自动分配占用。",
                "Complete rules save automatically. Custom aliases are reserved from automatic assignment."
            )),
            mtm,
        );
        message.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        message.setFrame(rect(30.0, 38.0, 640.0, 50.0));
        root.addSubview(&message);
        Self {
            window,
            document,
            scroll,
            rows: RefCell::default(),
            message,
        }
    }

    fn add_row(&self, target: &AnyObject, mtm: MainThreadMarker) -> Rc<Row> {
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 660.0, 86.0));
        let choose = button(
            tr!("选择应用…", "Choose App…"),
            target,
            sel!(chooseAliasApp:),
            rect(10.0, 48.0, 450.0, 28.0),
            mtm,
        );
        let remove = button(
            tr!("移除", "Remove"),
            target,
            sel!(removeAliasRule:),
            rect(555.0, 48.0, 95.0, 28.0),
            mtm,
        );
        view.addSubview(&choose);
        view.addSubview(&remove);
        let alias =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(15.0, 10.0, 70.0, 28.0));
        alias.setPlaceholderString(Some(ns_string!("ck")));
        alias.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "Alias 字母",
            "Alias letters"
        ))));
        let title =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(103.0, 10.0, 540.0, 28.0));
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
        let row = Rc::new(Row {
            view,
            choose,
            remove,
            alias,
            title,
            application: RefCell::default(),
        });
        self.document.addSubview(&row.view);
        self.rows.borrow_mut().push(row.clone());
        row
    }

    pub fn fill(&self, rules: &[AliasRule], target: &AnyObject, mtm: MainThreadMarker) {
        for row in self.rows.take() {
            row.view.removeFromSuperview();
        }
        for rule in rules {
            let row = self.add_row(target, mtm);
            row.set_application(rule.application.clone());
            row.alias.setStringValue(&NSString::from_str(&rule.alias));
            row.title
                .setStringValue(&NSString::from_str(&rule.title_contains));
        }
        self.layout();
    }

    pub fn copy_draft_from(&self, previous: &Self, target: &AnyObject, mtm: MainThreadMarker) {
        for old in previous.rows.borrow().iter() {
            let row = self.add_row(target, mtm);
            if let Some(app) = old.application.borrow().clone() {
                row.set_application(app);
            }
            row.alias.setStringValue(&old.alias.stringValue());
            row.title.setStringValue(&old.title.stringValue());
        }
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
        let row = self.add_row(target, mtm);
        self.layout();
        row.view.scrollRectToVisible(row.view.bounds());
    }

    pub fn remove(&self, index: usize) {
        if index < self.rows.borrow().len() {
            self.rows
                .borrow_mut()
                .remove(index)
                .view
                .removeFromSuperview();
            self.layout();
        }
    }

    fn layout(&self) {
        let rows = self.rows.borrow();
        let height = (rows.len() as f64 * 86.0).max(self.scroll.contentSize().height);
        self.document.setFrameSize(NSSize::new(660.0, height));
        for (index, row) in rows.iter().enumerate() {
            row.view
                .setFrameOrigin(NSPoint::new(0.0, height - (index + 1) as f64 * 86.0));
            row.choose.setTag(index as isize);
            row.remove.setTag(index as isize);
        }
    }

    pub fn choose(&self, index: usize, mtm: MainThreadMarker) {
        let Some(row) = self.rows.borrow().get(index).cloned() else {
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
                && let Some(url) = selected.URL()
            {
                match crate::app_shortcuts::target_at_url(&url) {
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
        panel.beginSheetModalForWindow_completionHandler(&self.window, &completion);
    }

    pub fn candidate(&self) -> Result<Vec<AliasRule>, String> {
        let rules: Vec<_> = self
            .rows
            .borrow()
            .iter()
            .filter_map(|row| {
                let application = row.application.borrow().clone()?;
                let alias = row.alias.stringValue().to_string().trim().to_string();
                if alias.is_empty() {
                    return None;
                }
                Some(AliasRule {
                    alias,
                    application,
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
