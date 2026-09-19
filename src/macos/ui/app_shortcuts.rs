use block2::RcBlock;
use std::cell::RefCell;
use std::rc::Rc;
use winlane::tr;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadOnly, sel};
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSArray, NSString, NSURL, ns_string};
use winlane::core::config::{AppShortcut, ApplicationTarget, Shortcut};

use crate::macos::platform::applications::target_at_url;
use crate::macos::ui::controls::{button, hint, label, preferences_window, rect};
use crate::macos::ui::shortcut::ShortcutControls;

struct AppShortcutRow {
    view: Retained<NSView>,
    application: RefCell<Option<ApplicationTarget>>,
    choose: Retained<NSButton>,
    remove: Retained<NSButton>,
    shortcut: ShortcutControls,
}

impl AppShortcutRow {
    fn set_application(&self, application: ApplicationTarget) {
        self.choose.setTitle(&NSString::from_str(&application.name));
        self.choose
            .setToolTip(Some(&NSString::from_str(&application.path)));
        self.application.replace(Some(application));
    }
}

pub struct AppShortcutsWindow {
    pub window: Retained<NSWindow>,
    document: Retained<NSView>,
    scroll: Retained<NSScrollView>,
    rows: RefCell<Vec<Rc<AppShortcutRow>>>,
    empty: Retained<NSTextField>,
    message: Retained<NSTextField>,
}

impl AppShortcutsWindow {
    pub fn new(target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let window = preferences_window(rect(0.0, 0.0, 700.0, 600.0), mtm);
        window.setTitle(&NSString::from_str(tr!(
            "Winlane · 应用快捷键",
            "Winlane · App Shortcuts"
        )));
        let root = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 700.0, 600.0));
        window.setContentView(Some(&root));
        root.addSubview(&label(
            tr!("应用快捷键", "App shortcuts"),
            23.0,
            rect(30.0, 547.0, 400.0, 32.0),
            mtm,
        ));
        root.addSubview(&hint(
            tr!(
                "按下组合键直接切换；应用未运行时自动启动。",
                "Switch directly with a shortcut; launch the app if it is not running."
            ),
            rect(30.0, 515.0, 640.0, 25.0),
            mtm,
        ));
        root.addSubview(&button(
            tr!("＋ 添加", "＋ Add"),
            target,
            sel!(addAppShortcut:),
            rect(555.0, 549.0, 115.0, 30.0),
            mtm,
        ));
        let scroll =
            NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(20.0, 119.0, 660.0, 386.0));
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setDrawsBackground(false);
        let document = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 660.0, 386.0));
        let empty = hint(
            tr!(
                "点击“添加”，选择应用并设置组合键。",
                "Click Add, choose an app, and assign a shortcut."
            ),
            rect(30.0, 310.0, 600.0, 26.0),
            mtm,
        );
        document.addSubview(&empty);
        scroll.setDocumentView(Some(&document));
        root.addSubview(&scroll);
        root.addSubview(&hint(
            tr!(
                "这些组合键全局生效，会替代应用原有的相同快捷键。",
                "These global shortcuts override the same key combinations in other apps."
            ),
            rect(30.0, 89.0, 640.0, 24.0),
            mtm,
        ));
        let message = NSTextField::wrappingLabelWithString(
            &NSString::from_str(tr!(
                "有效的快捷键自动保存，重启后保留。",
                "Valid shortcuts save automatically and persist after restarting Winlane."
            )),
            mtm,
        );
        message.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        message.setTextColor(Some(&NSColor::secondaryLabelColor()));
        message.setFrame(rect(30.0, 42.0, 640.0, 43.0));
        message.setMaximumNumberOfLines(2);
        root.addSubview(&message);
        Self {
            window,
            document,
            scroll,
            rows: RefCell::default(),
            empty,
            message,
        }
    }

    pub fn fill(&self, items: &[AppShortcut], target: &AnyObject, mtm: MainThreadMarker) {
        for row in self.rows.borrow_mut().drain(..) {
            row.view.removeFromSuperview();
        }
        for item in items {
            let row = self.add_row(target, mtm);
            row.shortcut.fill(&item.shortcut);
            row.set_application(item.application.clone());
        }
        self.layout();
        if let Some(row) = self.rows.borrow().first() {
            row.view.scrollRectToVisible(row.view.bounds());
        }
    }

    pub fn copy_draft_from(&self, previous: &Self, target: &AnyObject, mtm: MainThreadMarker) {
        self.fill(&[], target, mtm);
        for old in previous.rows.borrow().iter() {
            let row = self.add_row(target, mtm);
            row.shortcut.copy_from(&old.shortcut);
            if let Some(application) = old.application.borrow().clone() {
                row.set_application(application);
            }
        }
        self.layout();
    }

    pub fn show(&self, items: &[AppShortcut], target: &AnyObject, mtm: MainThreadMarker) {
        self.fill(items, target, mtm);
        self.report(
            tr!(
                "有效的快捷键自动保存，重启后保留。",
                "Valid shortcuts save automatically and persist after restarting Winlane."
            ),
            false,
        );
        self.window.center();
        self.window.makeKeyAndOrderFront(None);
    }

    fn add_row(&self, target: &AnyObject, mtm: MainThreadMarker) -> Rc<AppShortcutRow> {
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 660.0, 104.0));
        let choose = button(
            tr!("选择应用…", "Choose App…"),
            target,
            sel!(chooseShortcutApp:),
            rect(30.0, 65.0, 450.0, 28.0),
            mtm,
        );
        view.addSubview(&choose);
        let remove = button(
            tr!("移除", "Remove"),
            target,
            sel!(removeAppShortcut:),
            rect(535.0, 65.0, 95.0, 28.0),
            mtm,
        );
        view.addSubview(&remove);
        let shortcut = ShortcutControls::at(&view, 25.0, false, mtm);
        let key = (1..=10)
            .map(|digit| format!("Digit{}", digit % 10))
            .find(|key| {
                !self.rows.borrow().iter().any(|row| {
                    row.shortcut.read().is_ok_and(|shortcut| {
                        shortcut.command
                            && !shortcut.control
                            && !shortcut.option
                            && !shortcut.shift
                            && shortcut.key == *key
                    })
                })
            })
            .unwrap_or_else(|| "Digit1".into());
        shortcut.fill(&Shortcut {
            command: true,
            control: false,
            option: false,
            shift: false,
            key,
        });
        shortcut.on_change(target, sel!(appShortcutsChanged:));
        let row = Rc::new(AppShortcutRow {
            view,
            choose,
            remove,
            shortcut,
            application: RefCell::default(),
        });
        self.document.addSubview(&row.view);
        self.rows.borrow_mut().push(row.clone());
        row
    }

    pub fn add(&self, target: &AnyObject, mtm: MainThreadMarker) {
        if self.rows.borrow().len() >= 32 {
            self.report(
                tr!(
                    "最多设置 32 个应用快捷键。",
                    "You can configure up to 32 app shortcuts."
                ),
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
        let height = (rows.len() as f64 * 104.0).max(self.scroll.contentSize().height);
        self.document
            .setFrameSize(objc2_foundation::NSSize::new(660.0, height));
        self.empty.setHidden(!rows.is_empty());
        for (index, row) in rows.iter().enumerate() {
            row.view.setFrameOrigin(objc2_foundation::NSPoint::new(
                0.0,
                height - (index + 1) as f64 * 104.0,
            ));
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
        panel.setPrompt(Some(&NSString::from_str(tr!("选择应用", "Choose App"))));
        let selected_panel = panel.clone();
        let message = self.message.clone();
        let completion = RcBlock::new(move |response| {
            if response != NSModalResponseOK {
                return;
            }
            if let Some(url) = selected_panel.URL() {
                match target_at_url(&url) {
                    Ok(application) => {
                        row.set_application(application);
                        row.shortcut.notify_changed();
                    }
                    Err(error) => {
                        let text = NSString::from_str(&error);
                        message.setStringValue(&text);
                        message.setToolTip(Some(&text));
                        message.setTextColor(Some(&NSColor::systemRedColor()));
                    }
                }
            }
        });
        panel.beginSheetModalForWindow_completionHandler(&self.window, &completion);
    }

    pub fn candidate(&self) -> Result<Vec<AppShortcut>, String> {
        self.rows
            .borrow()
            .iter()
            .filter_map(|row| {
                let application = row.application.borrow().clone()?;
                Some(row.shortcut.read().map(|shortcut| AppShortcut {
                    shortcut,
                    application,
                }))
            })
            .collect()
    }

    pub fn report(&self, text: &str, error: bool) {
        let message = NSString::from_str(text);
        self.message.setStringValue(&message);
        self.message
            .setToolTip((!text.is_empty()).then_some(&message));
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
#[path = "../../../tests/native/app_shortcuts.rs"]
pub(crate) mod tests;
