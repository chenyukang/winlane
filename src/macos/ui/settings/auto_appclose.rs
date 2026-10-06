use super::*;
use block2::RcBlock;
use objc2_foundation::{NSArray, NSURL, ns_string};
use std::rc::Rc;
use winlane::{core::config::ApplicationTarget, features::auto_appclose::Rule};

fn optional_limit(field: &NSTextField, message: &str) -> Result<Option<u16>, String> {
    let text = field.stringValue().to_string();
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    text.parse().map(Some).map_err(|_| message.to_owned())
}

struct Row {
    view: Retained<NSView>,
    application: RefCell<Option<ApplicationTarget>>,
    choose: Retained<NSButton>,
    limit: Retained<NSTextField>,
    idle: Retained<NSTextField>,
    remove: Retained<NSButton>,
}

pub(super) struct AutoAppClosePage {
    enabled: Retained<NSButton>,
    interval: Retained<NSTextField>,
    grace: Retained<NSTextField>,
    scroll: Retained<NSScrollView>,
    document: Retained<NSView>,
    rows: RefCell<Vec<Rc<Row>>>,
    target: Weak<AnyObject>,
    status: Retained<NSTextField>,
}

impl AutoAppClosePage {
    pub(super) fn new(host: &NSView, target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let enabled = checkbox(tr!("启用自动关闭窗口", "Enable Auto AppClose"), mtm);
        enabled.setFrame(rect(4.0, 533.0, 730.0, 28.0));
        set_action(&enabled, target, sel!(toggleAutoAppClose:));
        host.addSubview(&enabled);
        host.addSubview(&hint(tr!("超过保留数时关闭最久未使用的窗口；闲置超过设定时间的窗口也会关闭。始终保留当前窗口，遇到保存提示时暂停该应用。", "Close the least recently used windows above each limit, and windows idle longer than their time. The active window is kept; a save prompt pauses that app."), rect(4.0, 480.0, 730.0, 44.0), mtm));
        host.addSubview(&label(
            tr!("检查间隔", "Check interval"),
            13.0,
            rect(4.0, 442.0, 144.0, 26.0),
            mtm,
        ));
        let interval =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(156.0, 442.0, 88.0, 30.0));
        interval.setFont(Some(&NSFont::systemFontOfSize(14.0)));
        interval.setAlignment(NSTextAlignment::Center);
        interval.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "检查间隔（秒）",
            "Check interval in seconds"
        ))));
        interval.cell().unwrap().setSendsActionOnEndEditing(true);
        set_action(&interval, target, sel!(settingsChanged:));
        host.addSubview(&interval);
        host.addSubview(&label(
            tr!("秒", "seconds"),
            13.0,
            rect(254.0, 442.0, 86.0, 26.0),
            mtm,
        ));
        let grace = hint("", rect(348.0, 440.0, 388.0, 32.0), mtm);
        host.addSubview(&grace);
        host.addSubview(&label(
            tr!("应用", "Application"),
            13.0,
            rect(10.0, 409.0, 296.0, 24.0),
            mtm,
        ));
        host.addSubview(&label(
            tr!("保留窗口数", "Keep windows"),
            13.0,
            rect(308.0, 409.0, 116.0, 24.0),
            mtm,
        ));
        host.addSubview(&label(
            tr!("闲置（分钟）", "Idle (minutes)"),
            13.0,
            rect(432.0, 409.0, 116.0, 24.0),
            mtm,
        ));
        let scroll =
            NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(0.0, 66.0, 740.0, 338.0));
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setScrollerStyle(NSScrollerStyle::Overlay);
        scroll.setDrawsBackground(false);
        let document = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 740.0, 338.0));
        scroll.setDocumentView(Some(&document));
        host.addSubview(&scroll);
        host.addSubview(&button(
            tr!("＋ 添加应用…", "＋ Add App…"),
            target,
            sel!(addAppCloseRule:),
            rect(0.0, 16.0, 200.0, 30.0),
            mtm,
        ));
        let status = hint("", rect(216.0, 4.0, 520.0, 52.0), mtm);
        status.setMaximumNumberOfLines(3);
        host.addSubview(&status);
        Self {
            enabled,
            interval,
            grace,
            scroll,
            document,
            rows: RefCell::default(),
            target: Weak::new(target),
            status,
        }
    }

    pub(super) fn fill(&self, config: &Config) {
        self.interval.setStringValue(&NSString::from_str(
            &config.auto_appclose.interval_secs.to_string(),
        ));
        self.update_grace(config);
        self.enabled.setState(if config.auto_appclose.enabled {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        for row in self.rows.take() {
            row.view.removeFromSuperview();
        }
        for rule in &config.auto_appclose.rules {
            let row = self.append();
            Self::set_application(&row, rule.application.clone());
            row.limit.setStringValue(&NSString::from_str(
                &rule
                    .max_windows
                    .map_or(String::new(), |max| max.to_string()),
            ));
        }
        self.layout();
    }

    pub(super) fn read(&self, config: &mut Config) -> Result<(), String> {
        config.auto_appclose.interval_secs = self
            .interval
            .stringValue()
            .to_string()
            .trim()
            .parse()
            .map_err(|_| {
                tr!(
                    "检查间隔应为 1–3600 秒的整数。",
                    "Check interval must be an integer from 1 to 3600 seconds."
                )
            })?;
        let mut rules = Vec::new();
        for row in self.rows.borrow().iter() {
            if let Some(application) = row.application.borrow().clone() {
                rules.push(Rule {
                    application,
                    max_windows: optional_limit(
                        &row.limit,
                        tr!(
                            "保留窗口数应为 1–100 的整数，留空表示不限制。",
                            "Keep between 1 and 100 windows, or leave it empty for no limit."
                        ),
                    )?,
                    max_idle_minutes: optional_limit(
                        &row.idle,
                        tr!(
                            "闲置时间应为 1–10080 分钟的整数，留空表示不按闲置关闭。",
                            "Idle time must be an integer from 1 to 10080 minutes, or empty to never close for being idle."
                        ),
                    )?,
                });
            }
        }
        config.auto_appclose.enabled = self.enabled.state() == NSControlStateValueOn;
        config.auto_appclose.rules = rules;
        config.auto_appclose.validate()
    }

    pub(super) fn update_grace(&self, config: &Config) {
        self.grace.setStringValue(&NSString::from_str(&trf!(
            "新窗口保护期：{} 秒（2×间隔）",
            "New window grace: {} seconds (2× interval)",
            config.auto_appclose.grace_period_ms() / 1_000
        )));
    }

    fn append(&self) -> Rc<Row> {
        let mtm = self.document.mtm();
        let target = self.target.load().expect("settings target is alive");
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 740.0, 54.0));
        let choose = button(
            tr!("选择应用…", "Choose App…"),
            &target,
            sel!(chooseAppCloseApp:),
            rect(4.0, 12.0, 296.0, 30.0),
            mtm,
        );
        choose.setAlignment(NSTextAlignment::Left);
        view.addSubview(&choose);
        let limit =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(308.0, 12.0, 116.0, 30.0));
        limit.setFont(Some(&NSFont::systemFontOfSize(14.0)));
        limit.setStringValue(ns_string!("3"));
        limit.setAlignment(NSTextAlignment::Center);
        limit.setAccessibilityLabel(Some(&NSString::from_str(tr!("保留窗口数", "Keep windows"))));
        limit.cell().unwrap().setSendsActionOnEndEditing(true);
        set_action(&limit, &target, sel!(settingsChanged:));
        view.addSubview(&limit);
        let idle =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(432.0, 12.0, 116.0, 30.0));
        idle.setFont(Some(&NSFont::systemFontOfSize(14.0)));
        idle.setAlignment(NSTextAlignment::Center);
        idle.setPlaceholderString(Some(&NSString::from_str(tr!("不做限制", "No limit"))));
        idle.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "闲置多少分钟后关闭（留空则不限）",
            "Close after this many idle minutes, empty for no limit"
        ))));
        idle.setToolTip(Some(&NSString::from_str(tr!(
            "窗口超过该时间未被使用就关闭；留空表示不按闲置关闭。",
            "Close a window this long after it was last used. Leave empty to never close for being idle."
        ))));
        idle.cell().unwrap().setSendsActionOnEndEditing(true);
        set_action(&idle, &target, sel!(settingsChanged:));
        view.addSubview(&idle);
        let remove = button(
            tr!("移除", "Remove"),
            &target,
            sel!(removeAppCloseRule:),
            rect(610.0, 12.0, 116.0, 30.0),
            mtm,
        );
        view.addSubview(&remove);
        let row = Rc::new(Row {
            view,
            application: RefCell::default(),
            choose,
            limit,
            idle,
            remove,
        });
        self.document.addSubview(&row.view);
        self.rows.borrow_mut().push(row.clone());
        row
    }
    fn set_application(row: &Row, application: ApplicationTarget) {
        row.choose.setTitle(&NSString::from_str(&application.name));
        row.choose
            .setToolTip(Some(&NSString::from_str(&application.bundle_id)));
        row.application.replace(Some(application));
    }
    fn layout(&self) {
        let rows = self.rows.borrow();
        let height = (rows.len() as f64 * 54.0).max(self.scroll.contentSize().height);
        self.document.setFrameSize(NSSize::new(740.0, height));
        for (i, row) in rows.iter().enumerate() {
            row.view
                .setFrameOrigin(NSPoint::new(0.0, height - (i + 1) as f64 * 54.0));
            row.choose.setTag(i as isize);
            row.remove.setTag(i as isize);
        }
    }
    pub(super) fn add(&self, window: &NSWindow, message: &Retained<NSTextField>) {
        if self.rows.borrow().len() >= 64 {
            return;
        }
        let row = self.append();
        self.layout();
        row.view.scrollRectToVisible(row.view.bounds());
        let index = self.rows.borrow().len() - 1;
        self.choose(index, window, message);
    }
    pub(super) fn remove(&self, index: usize) {
        if index < self.rows.borrow().len() {
            self.rows
                .borrow_mut()
                .remove(index)
                .view
                .removeFromSuperview();
            self.layout();
        }
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
                match crate::macos::platform::applications::target_at_url(&url) {
                    Ok(application) => {
                        Self::set_application(&row, application);
                        // SAFETY: The control's action targets the live settings delegate.
                        unsafe {
                            row.limit
                                .sendAction_to(row.limit.action(), row.limit.target().as_deref());
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
    pub(super) fn status(&self, text: &str) {
        if self.status.stringValue().to_string() != text {
            self.status.setStringValue(&NSString::from_str(text));
        }
    }
    pub(super) fn enabled(&self) -> bool {
        self.enabled.state() == NSControlStateValueOn
    }
}

#[cfg(test)]
#[path = "../../../../tests/native/settings_auto_appclose.rs"]
pub(crate) mod tests;
