use crate::macos::ui::controls::input;
use crate::macos::ui::controls::{button, hint, label, rect};
use crate::macos::ui::shortcut::ShortcutControls;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::*;
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSSize, NSString, NSUUID,
};
use std::cell::{Cell, OnceCell, RefCell};
use std::io::Read;
use winlane::features::quicklinks::{self, Quicklink};
use winlane::{tr, trf};

type Save = Box<dyn Fn(Vec<Quicklink>) -> Result<(), String>>;
pub(crate) struct EditorState {
    ui: OnceCell<EditorUi>,
    links: RefCell<Vec<Quicklink>>,
    saved: RefCell<Vec<Quicklink>>,
    selected: Cell<Option<usize>>,
    filling: Cell<bool>,
    save: Save,
}
struct EditorUi {
    root: Retained<NSView>,
    list: Retained<NSView>,
    scroll: Retained<NSScrollView>,
    name: Retained<NSTextField>,
    link: Retained<NSTextField>,
    application: Retained<NSTextField>,
    shortcut: ShortcutControls,
    remove: Retained<NSButton>,
    message: Retained<NSTextField>,
}

define_class!(
    // SAFETY: AppKit controls and callbacks stay on the main thread.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = EditorState]
    pub(crate) struct QuicklinkEditor;
    unsafe impl NSObjectProtocol for QuicklinkEditor {}
    unsafe impl NSTextFieldDelegate for QuicklinkEditor {}
    unsafe impl NSControlTextEditingDelegate for QuicklinkEditor {
        #[unsafe(method(controlTextDidChange:))]
        fn changed(&self, _: &NSNotification) { self.input_changed(); }
    }
    impl QuicklinkEditor {
        #[unsafe(method(quicklinkShortcutChanged:))]
        fn shortcut_changed(&self, _: Option<&AnyObject>) { self.input_changed(); }
        #[unsafe(method(selectQuicklink:))]
        fn select(&self, sender: &NSButton) { self.ivars().selected.set(Some(sender.tag() as usize)); self.fill(); }
        #[unsafe(method(addQuicklink:))]
        fn add(&self, _: Option<&AnyObject>) {
            let mut links = self.ivars().links.borrow_mut();
            if links.len() >= quicklinks::MAX_LINKS { self.report(tr!("最多保存 200 个快捷链接。", "You can save up to 200 quicklinks.")); return; }
            let index = links.iter().position(|q| q.name.is_empty() && q.link.is_empty()).unwrap_or_else(|| {
                links.push(Quicklink { id: NSUUID::UUID().UUIDString().to_string(), name: String::new(), link: String::new(), open_with: String::new(), shortcut: None }); links.len() - 1
            });
            drop(links); self.ivars().selected.set(Some(index)); self.fill();
            if let Some(window) = self.view().window() { window.makeFirstResponder(Some(&*self.ui().name)); }
        }
        #[unsafe(method(deleteQuicklink:))]
        fn delete(&self, _: Option<&AnyObject>) {
            let Some(index) = self.ivars().selected.get() else { return; };
            self.ivars().links.borrow_mut().remove(index);
            let count = self.ivars().links.borrow().len();
            self.ivars().selected.set(if count == 0 { None } else { Some(index.min(count - 1)) });
            self.persist(); self.fill();
        }
        #[unsafe(method(importQuicklinks:))]
        fn import(&self, _: Option<&AnyObject>) {
            let panel = NSOpenPanel::openPanel(self.mtm());
            panel.setCanChooseDirectories(false); panel.setAllowsMultipleSelection(false);
            panel.setTitle(Some(&NSString::from_str(tr!("导入 Raycast Quicklinks", "Import Raycast Quicklinks"))));
            if panel.runModal() != NSModalResponseOK { return; }
            if let Some(path) = panel.URL().and_then(|url| url.path()) {
                match self.import_file(std::path::Path::new(&path.to_string())) {
                    Ok((added, skipped)) => self.report(&trf!("已导入 {} 个，跳过 {} 个重复链接。", "Imported {}; skipped {} duplicates.", added, skipped)),
                    Err(error) => self.report(&error),
                }
            }
        }
    }
);

impl QuicklinkEditor {
    pub fn new(links: Vec<Quicklink>, save: Save, mtm: MainThreadMarker) -> Retained<Self> {
        let selected = if links.is_empty() { None } else { Some(0) };
        let this = Self::alloc(mtm).set_ivars(EditorState {
            ui: OnceCell::new(),
            saved: RefCell::new(links.clone()),
            links: RefCell::new(links),
            selected: Cell::new(selected),
            filling: Cell::new(false),
            save,
        });
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        let root = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 740.0, 574.0));
        root.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        let scroll =
            NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(12.0, 56.0, 180.0, 468.0));
        scroll.setAutoresizingMask(NSAutoresizingMaskOptions::ViewHeightSizable);
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setDrawsBackground(false);
        scroll.setScrollerStyle(NSScrollerStyle::Overlay);
        let list = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 180.0, 468.0));
        scroll.setDocumentView(Some(&list));
        root.addSubview(&scroll);
        root.addSubview(&button(
            tr!("＋ 新建", "＋ New"),
            &this,
            sel!(addQuicklink:),
            rect(10.0, 12.0, 90.0, 30.0),
            mtm,
        ));
        let remove = button(
            tr!("删除", "Delete"),
            &this,
            sel!(deleteQuicklink:),
            rect(106.0, 12.0, 88.0, 30.0),
            mtm,
        );
        root.addSubview(&remove);
        let mut fields = Vec::new();
        for (title, placeholder, y) in [
            (
                tr!("名称", "Name"),
                tr!("快捷链接名称", "Quicklink name"),
                498.0,
            ),
            (
                tr!("链接", "Link"),
                "https://example.com/search?q={Query}",
                408.0,
            ),
            (
                tr!("打开方式", "Open with"),
                tr!(
                    "默认应用（可填写应用名称或 bundle ID）",
                    "Default app (or enter an app name / bundle ID)"
                ),
                318.0,
            ),
        ] {
            let caption = label(title, 13.0, rect(236.0, y + 32.0, 488.0, 22.0), mtm);
            caption.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
            root.addSubview(&caption);
            let field = input(rect(236.0, y, 488.0, 30.0), placeholder, mtm);
            field.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewMinYMargin
                    | NSAutoresizingMaskOptions::ViewWidthSizable,
            );
            unsafe {
                field.setDelegate(Some(ProtocolObject::from_ref(&*this)));
            }
            root.addSubview(&field);
            fields.push(field);
        }
        let name = fields.remove(0);
        let link = fields.remove(0);
        let application = fields.remove(0);
        let caption = label(
            tr!("全局快捷键", "Global shortcut"),
            13.0,
            rect(236.0, 276.0, 488.0, 22.0),
            mtm,
        );
        caption.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        root.addSubview(&caption);
        let shortcut = ShortcutControls::optional_at(&root, 236.0, 244.0, mtm);
        shortcut.on_change(&this, sel!(quicklinkShortcutChanged:));
        let help = hint(
            tr!(
                "支持网址、应用链接、/绝对路径与 ~/路径。\n{Query} 输入参数 · {clipboard} 剪贴板\n网址参数自动编码；{clipboard | raw} 保留原文。",
                "URLs, app links, /absolute paths and ~/paths.\n{Query} prompts for input · {clipboard} uses copied text\nURL values are encoded; {clipboard | raw} keeps them unchanged."
            ),
            rect(236.0, 134.0, 488.0, 96.0),
            mtm,
        );
        help.setMaximumNumberOfLines(0);
        root.addSubview(&help);
        help.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewMinYMargin | NSAutoresizingMaskOptions::ViewWidthSizable,
        );
        let message = hint("", rect(236.0, 52.0, 488.0, 76.0), mtm);
        root.addSubview(&message);
        root.addSubview(&button(
            tr!("导入 Raycast JSON…", "Import Raycast JSON…"),
            &this,
            sel!(importQuicklinks:),
            rect(490.0, 20.0, 234.0, 30.0),
            mtm,
        ));
        crate::macos::ui::controls::editor_chrome(&root, mtm);
        this.ivars()
            .ui
            .set(EditorUi {
                root,
                list,
                scroll,
                name,
                link,
                application,
                shortcut,
                remove,
                message,
            })
            .ok()
            .unwrap();
        this.fill();
        this
    }
    fn ui(&self) -> &EditorUi {
        self.ivars().ui.get().unwrap()
    }
    pub fn view(&self) -> &NSView {
        &self.ui().root
    }
    pub fn copy_draft_from(&self, old: &Self) {
        self.ivars()
            .links
            .replace(old.ivars().links.borrow().clone());
        self.ivars()
            .saved
            .replace(old.ivars().saved.borrow().clone());
        self.ivars().selected.set(old.ivars().selected.get());
        self.fill();
    }
    fn report(&self, message: &str) {
        let text = NSString::from_str(message);
        self.ui().message.setStringValue(&text);
        self.ui()
            .message
            .setToolTip((!message.is_empty()).then_some(&text));
    }
    fn fill(&self) {
        self.ivars().filling.set(true);
        let link = self
            .ivars()
            .selected
            .get()
            .and_then(|i| self.ivars().links.borrow().get(i).cloned());
        for (field, value) in [
            (
                &self.ui().name,
                link.as_ref().map_or("", |q| q.name.as_str()),
            ),
            (
                &self.ui().link,
                link.as_ref().map_or("", |q| q.link.as_str()),
            ),
            (
                &self.ui().application,
                link.as_ref().map_or("", |q| q.open_with.as_str()),
            ),
        ] {
            field.setEnabled(link.is_some());
            field.setStringValue(&NSString::from_str(value));
        }
        self.ui().remove.setEnabled(link.is_some());
        self.ui()
            .shortcut
            .fill_optional(link.as_ref().and_then(|link| link.shortcut.as_ref()));
        self.ui().shortcut.set_enabled(link.is_some());
        self.ivars().filling.set(false);
        self.rebuild_list();
    }
    fn rebuild_list(&self) {
        let ui = self.ui();
        for child in ui.list.subviews() {
            child.removeFromSuperview();
        }
        let links = self.ivars().links.borrow();
        let height = (links.len() as f64 * 36.0).max(ui.scroll.contentSize().height);
        let width = ui.scroll.contentSize().width;
        ui.list.setFrameSize(NSSize::new(width, height));
        for (i, link) in links.iter().enumerate() {
            let row = button(
                if link.name.is_empty() {
                    tr!("未命名链接", "Untitled quicklink")
                } else {
                    &link.name
                },
                self,
                sel!(selectQuicklink:),
                rect(0.0, height - (i + 1) as f64 * 36.0, width, 32.0),
                self.mtm(),
            );
            row.setTag(i as isize);
            row.setButtonType(NSButtonType::Toggle);
            row.setBezelStyle(NSBezelStyle::AccessoryBarAction);
            row.setAlignment(NSTextAlignment::Left);
            row.setState(if self.ivars().selected.get() == Some(i) {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
            ui.list.addSubview(&row);
        }
    }
    fn input_changed(&self) {
        if self.ivars().filling.get() {
            return;
        }
        if let Some(i) = self.ivars().selected.get() {
            let mut links = self.ivars().links.borrow_mut();
            links[i].name = self.ui().name.stringValue().to_string();
            links[i].link = self.ui().link.stringValue().to_string();
            links[i].open_with = self.ui().application.stringValue().to_string();
            match self.ui().shortcut.read_optional() {
                Ok(shortcut) => links[i].shortcut = shortcut,
                Err(error) => {
                    self.report(&error);
                    return;
                }
            }
        }
        self.persist();
        self.rebuild_list();
    }
    fn persist(&self) {
        let mut candidate = Vec::new();
        let mut error = None;
        for draft in self.ivars().links.borrow().iter() {
            if let Err(problem) = draft.validate() {
                error.get_or_insert(problem);
                if let Some(saved) = self
                    .ivars()
                    .saved
                    .borrow()
                    .iter()
                    .find(|saved| saved.id == draft.id)
                {
                    candidate.push(saved.clone());
                }
            } else {
                candidate.push(draft.clone());
            }
        }
        match quicklinks::validate(&candidate).and_then(|()| (self.ivars().save)(candidate.clone()))
        {
            Ok(()) => {
                self.ivars().saved.replace(candidate);
                self.report(&error.map_or(String::new(), |e| {
                    trf!("未保存当前草稿：{}", "Draft not saved: {}", e)
                }));
            }
            Err(error) => self.report(&error),
        }
    }
    pub fn import_file(&self, path: &std::path::Path) -> Result<(usize, usize), String> {
        let mut json = String::new();
        std::fs::File::open(path)
            .and_then(|f| {
                f.take(quicklinks::MAX_IMPORT_BYTES as u64 + 1)
                    .read_to_string(&mut json)
            })
            .map_err(|_| tr!("无法读取导入文件。", "Could not read the import file.").to_owned())?;
        let imported = quicklinks::import_json(&self.ivars().saved.borrow(), &json)?;
        (self.ivars().save)(imported.links.clone())?;
        // Keep unfinished edits in the editor while adding imported entries to saved state.
        let saved_ids: std::collections::HashSet<_> = self
            .ivars()
            .saved
            .borrow()
            .iter()
            .map(|q| q.id.clone())
            .collect();
        self.ivars().links.borrow_mut().extend(
            imported
                .links
                .iter()
                .filter(|q| !saved_ids.contains(&q.id))
                .cloned(),
        );
        self.ivars().saved.replace(imported.links);
        if self.ivars().selected.get().is_none() && !self.ivars().links.borrow().is_empty() {
            self.ivars().selected.set(Some(0));
        }
        self.fill();
        Ok((imported.added, imported.skipped))
    }
}

impl Drop for EditorState {
    fn drop(&mut self) {
        if let Some(ui) = self.ui.get() {
            for field in [&ui.name, &ui.link, &ui.application] {
                // SAFETY: Clear weak native delegates before the controller is released.
                unsafe {
                    field.setDelegate(None);
                }
            }
            ui.root.removeFromSuperview();
        }
    }
}

pub(crate) mod input;

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../../tests/native/quicklink_ui.rs"]
pub(crate) mod tests;
