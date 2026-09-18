use crate::settings::{button, hint, label, preferences_window, rect};
use crate::snippet_placeholder::{BuilderMode, PlaceholderBuilder};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::*;
use objc2_foundation::{
    MainThreadMarker, NSDate, NSDateFormatter, NSDateFormatterStyle, NSNotification, NSObject,
    NSObjectProtocol, NSSize, NSString, NSUUID, ns_string,
};
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use winlane::snippets::{ArgumentKind, Snippet, Template};
use winlane::{tr, trf};

type Save = Box<dyn Fn(Vec<Snippet>) -> Result<(), String>>;

pub(crate) struct EditorState {
    ui: OnceCell<EditorUi>,
    snippets: RefCell<Vec<Snippet>>,
    saved: RefCell<Vec<Snippet>>,
    selected: Cell<Option<usize>>,
    filling: Cell<bool>,
    insertion: Cell<objc2_foundation::NSRange>,
    save: Save,
}

struct EditorUi {
    root: Retained<NSView>,
    list: Retained<NSView>,
    scroll: Retained<NSScrollView>,
    name: Retained<NSTextField>,
    body: Retained<NSTextView>,
    preview: Retained<NSTextView>,
    placeholder: Retained<NSPopUpButton>,
    remove: Retained<NSButton>,
    message: Retained<NSTextField>,
    builder: PlaceholderBuilder,
    editing_views: Vec<Retained<NSView>>,
}

define_class!(
    // SAFETY: Controls and their delegate are retained and accessed on the main thread.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = EditorState]
    pub(crate) struct SnippetEditor;
    unsafe impl NSObjectProtocol for SnippetEditor {}
    unsafe impl NSTextFieldDelegate for SnippetEditor {}
    unsafe impl NSControlTextEditingDelegate for SnippetEditor {
        #[unsafe(method(controlTextDidChange:))]
        fn name_changed(&self, notification: &NSNotification) { self.input_changed(notification); }
    }
    unsafe impl NSTextViewDelegate for SnippetEditor {}
    unsafe impl NSTextDelegate for SnippetEditor {
        #[unsafe(method(textDidChange:))]
        fn body_changed(&self, notification: &NSNotification) { self.input_changed(notification); }
    }
    impl SnippetEditor {
        #[unsafe(method(selectSnippet:))]
        fn select(&self, sender: &NSButton) { self.ivars().selected.set(Some(sender.tag() as usize)); self.fill(); }
        #[unsafe(method(addSnippet:))]
        fn add(&self, _: Option<&AnyObject>) {
            if self.ivars().snippets.borrow().len() >= 200 { self.report(tr!("最多保存 200 个片段。", "You can save up to 200 snippets.")); return; }
            let empty = self.ivars().snippets.borrow().iter().position(|s| s.name.is_empty() && s.body.is_empty());
            if let Some(index) = empty {
                self.ivars().selected.set(Some(index));
            } else {
                let mut snippets = self.ivars().snippets.borrow_mut();
                let index = snippets.len();
                snippets.push(Snippet { id: NSUUID::UUID().UUIDString().to_string(), name: String::new(), body: String::new() });
                self.ivars().selected.set(Some(index));
            }
            self.fill();
            let ui = self.ui(); if let Some(window) = ui.root.window() { window.makeFirstResponder(Some(&*ui.name)); }
        }
        #[unsafe(method(deleteSnippet:))]
        fn delete(&self, _: Option<&AnyObject>) {
            let Some(index) = self.ivars().selected.get() else { return; };
            self.ivars().snippets.borrow_mut().remove(index);
            let count = self.ivars().snippets.borrow().len();
            self.ivars().selected.set(if count == 0 { None } else { Some(index.min(count - 1)) });
            self.persist(); self.fill();
        }
        #[unsafe(method(insertPlaceholder:))]
        fn insert_placeholder(&self, _: Option<&AnyObject>) {
            let ui = self.ui();
            let index = ui.placeholder.indexOfSelectedItem(); ui.placeholder.selectItemAtIndex(0);
            self.ivars().insertion.set(NSTextInputClient::selectedRange(&*ui.body));
            match index {
                1 => self.insert_token("{clipboard}"),
                2 | 3 => {
                    let arguments = Template::parse(&ui.body.string().to_string()).map(|template| template.arguments).unwrap_or_default();
                    self.set_building(true);
                    ui.builder.begin(if index == 2 { BuilderMode::Date } else { BuilderMode::Field }, &arguments);
                }
                _ => {}
            }
        }
        #[unsafe(method(placeholderBuilderChanged:))]
        fn builder_changed(&self, _: Option<&AnyObject>) { self.ui().builder.update(); }
        #[unsafe(method(reuseSnippetField:))]
        fn reuse_field(&self, _: Option<&AnyObject>) { self.ui().builder.reuse(); }
        #[unsafe(method(commitSnippetPlaceholder:))]
        fn commit_placeholder(&self, _: Option<&AnyObject>) {
            match self.ui().builder.token() { Ok(token) => self.insert_token(&token), Err(error) => self.ui().builder.report(&error) }
        }
        #[unsafe(method(cancelSnippetPlaceholder:))]
        fn cancel_placeholder(&self, _: Option<&AnyObject>) {
            self.set_building(false);
            if let Some(window) = self.ui().root.window() { window.makeFirstResponder(Some(&*self.ui().body)); }
        }

    }
);

impl SnippetEditor {
    pub fn new(snippets: Vec<Snippet>, save: Save, mtm: MainThreadMarker) -> Retained<Self> {
        let selected = if snippets.is_empty() { None } else { Some(0) };
        let this = Self::alloc(mtm).set_ivars(EditorState {
            ui: OnceCell::new(),
            saved: RefCell::new(snippets.clone()),
            snippets: RefCell::new(snippets),
            selected: Cell::new(selected),
            filling: Cell::new(false),
            insertion: Cell::new(objc2_foundation::NSRange::new(0, 0)),
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
            sel!(addSnippet:),
            rect(10.0, 12.0, 90.0, 30.0),
            mtm,
        ));
        let remove = button(
            tr!("删除", "Delete"),
            &this,
            sel!(deleteSnippet:),
            rect(106.0, 12.0, 88.0, 30.0),
            mtm,
        );
        root.addSubview(&remove);
        let name_label = label(
            tr!("名称", "Name"),
            13.0,
            rect(236.0, 538.0, 65.0, 24.0),
            mtm,
        );
        name_label.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        root.addSubview(&name_label);
        let name = input(
            rect(304.0, 536.0, 420.0, 28.0),
            tr!("片段名称", "Snippet name"),
            mtm,
        );
        unsafe {
            name.setDelegate(Some(ProtocolObject::from_ref(&*this)));
        }
        name.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewMinYMargin | NSAutoresizingMaskOptions::ViewWidthSizable,
        );
        root.addSubview(&name);
        let (body_scroll, body) = text_area(rect(236.0, 254.0, 488.0, 262.0), true, mtm);
        body_scroll.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewHeightSizable
                | NSAutoresizingMaskOptions::ViewWidthSizable,
        );
        body.setDelegate(Some(ProtocolObject::from_ref(&*this)));
        root.addSubview(&body_scroll);
        body.setAccessibilityLabel(Some(&NSString::from_str(tr!("片段正文", "Snippet text"))));
        let placeholder = NSPopUpButton::initWithFrame_pullsDown(
            NSPopUpButton::alloc(mtm),
            rect(236.0, 214.0, 280.0, 30.0),
            false,
        );
        for title in [
            tr!("插入占位符…", "Insert Placeholder…"),
            tr!("剪贴板", "Clipboard"),
            tr!("日期与时间…", "Date & Time…"),
            tr!("自定义输入字段…", "Custom Input Field…"),
        ] {
            placeholder.addItemWithTitle(&NSString::from_str(title));
        }
        unsafe {
            placeholder.setTarget(Some(&this));
            placeholder.setAction(Some(sel!(insertPlaceholder:)));
        }
        root.addSubview(&placeholder);
        root.addSubview(&hint(tr!("动态值在使用时展开。同名输入字段只需填写一次。\n输入 \\{date} 可保留字面量 {date}。", "Dynamic values expand when used. Repeated fields share one value.\nUse \\{date} to insert the literal text {date}."), rect(236.0, 165.0, 488.0, 43.0), mtm));
        root.addSubview(&label(
            tr!("预览", "Preview"),
            13.0,
            rect(236.0, 142.0, 488.0, 22.0),
            mtm,
        ));
        let (preview_scroll, preview) = text_area(rect(236.0, 64.0, 488.0, 70.0), false, mtm);
        root.addSubview(&preview_scroll);
        let message = hint("", rect(236.0, 16.0, 488.0, 38.0), mtm);
        root.addSubview(&message);
        let editing_views = root
            .subviews()
            .iter()
            .filter(|view| view.frame().origin.x >= 236.0 && view.frame().origin.y < 536.0)
            .collect();
        let builder = PlaceholderBuilder::new(&this, mtm);
        builder
            .view
            .setFrameOrigin(objc2_foundation::NSPoint::new(236.0, 64.0));
        root.addSubview(&builder.view);
        crate::settings::editor_chrome(&root, mtm);
        this.ivars()
            .ui
            .set(EditorUi {
                root,
                list,
                scroll,
                name,
                body,
                preview,
                placeholder,
                remove,
                message,
                builder,
                editing_views,
            })
            .ok()
            .unwrap();
        this.fill();
        this
    }

    fn input_changed(&self, notification: &NSNotification) {
        if notification
            .object()
            .is_some_and(|object| self.ui().builder.handles(&object))
        {
            self.ui().builder.update();
        } else {
            self.changed();
        }
    }
    fn set_building(&self, active: bool) {
        for view in &self.ui().editing_views {
            view.setHidden(active);
        }
        self.ui().builder.view.setHidden(!active);
    }
    fn insert_token(&self, token: &str) {
        let ui = self.ui();
        let range = self.ivars().insertion.get();
        match winlane::snippets::inserting(
            &ui.body.string().to_string(),
            range.location,
            range.length,
            token,
        ) {
            Ok(_) => {
                self.set_building(false);
                if let Some(window) = ui.root.window() {
                    window.makeFirstResponder(Some(&*ui.body));
                }
                unsafe {
                    ui.body
                        .insertText_replacementRange(&NSString::from_str(token), range);
                }
                self.changed();
            }
            Err(error) => {
                if ui.builder.view.isHidden() {
                    self.report(&error);
                } else {
                    ui.builder.report(&error);
                }
            }
        }
    }

    pub fn copy_draft_from(&self, previous: &Self) {
        self.ivars()
            .snippets
            .replace(previous.ivars().snippets.borrow().clone());
        self.ivars()
            .saved
            .replace(previous.ivars().saved.borrow().clone());
        self.ivars().selected.set(previous.ivars().selected.get());
        self.fill();
    }
    fn ui(&self) -> &EditorUi {
        self.ivars().ui.get().unwrap()
    }
    pub fn view(&self) -> &NSView {
        &self.ui().root
    }
    fn report(&self, error: &str) {
        self.ui().message.setStringValue(&NSString::from_str(error));
        self.ui()
            .message
            .setTextColor(Some(&NSColor::systemRedColor()));
    }

    fn fill(&self) {
        self.set_building(false);
        self.ivars().filling.set(true);
        let snippet = self
            .ivars()
            .selected
            .get()
            .and_then(|i| self.ivars().snippets.borrow().get(i).cloned());
        let ui = self.ui();
        ui.name.setEnabled(snippet.is_some());
        ui.body.setEditable(snippet.is_some());
        ui.placeholder.setEnabled(snippet.is_some());
        ui.remove.setEnabled(snippet.is_some());
        ui.name.setStringValue(&NSString::from_str(
            snippet.as_ref().map_or("", |s| &s.name),
        ));
        ui.body.setString(&NSString::from_str(
            snippet.as_ref().map_or("", |s| &s.body),
        ));
        ui.message.setStringValue(ns_string!(""));
        self.ivars().filling.set(false);
        self.rebuild_list();
        self.preview();
    }
    fn rebuild_list(&self) {
        let ui = self.ui();
        for child in ui.list.subviews() {
            child.removeFromSuperview();
        }
        let snippets = self.ivars().snippets.borrow();
        let height = (snippets.len() as f64 * 36.0).max(ui.scroll.contentSize().height);
        let width = ui.scroll.contentSize().width;
        ui.list.setFrameSize(NSSize::new(width, height));
        for (index, snippet) in snippets.iter().enumerate() {
            let name = if snippet.name.is_empty() {
                tr!("未命名片段", "Untitled snippet")
            } else {
                &snippet.name
            };
            let row = button(
                name,
                self,
                sel!(selectSnippet:),
                rect(0.0, height - (index + 1) as f64 * 36.0, width, 32.0),
                self.mtm(),
            );
            row.setTag(index as isize);
            row.setButtonType(NSButtonType::Toggle);
            row.setBezelStyle(NSBezelStyle::AccessoryBarAction);
            row.setAlignment(NSTextAlignment::Left);
            row.setState(if self.ivars().selected.get() == Some(index) {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
            ui.list.addSubview(&row);
        }
    }
    fn changed(&self) {
        if self.ivars().filling.get() {
            return;
        }
        if let Some(index) = self.ivars().selected.get() {
            let mut snippets = self.ivars().snippets.borrow_mut();
            snippets[index].name = self.ui().name.stringValue().to_string();
            snippets[index].body = self.ui().body.string().to_string();
        }
        self.persist();
        self.preview();
        self.rebuild_list();
    }
    fn persist(&self) {
        let mut candidate = Vec::new();
        let mut error = None;
        for draft in self.ivars().snippets.borrow().iter() {
            match draft.validate() {
                Ok(()) => candidate.push(draft.clone()),
                Err(problem) => {
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
                }
            }
        }
        let result = winlane::snippets::validate(&candidate)
            .and_then(|()| (self.ivars().save)(candidate.clone()));
        match result {
            Ok(()) => {
                self.ivars().saved.replace(candidate);
                self.report(&error.map_or(String::new(), |error| {
                    trf!("未保存当前草稿：{}", "Draft not saved: {}", error)
                }));
            }
            Err(error) => self.report(&trf!("未保存：{}", "Not saved: {}", error)),
        }
    }
    fn preview(&self) {
        let body = self.ui().body.string().to_string();
        let result = Template::parse(&body).and_then(|template| {
            let values = template
                .arguments
                .iter()
                .map(|arg| (arg.name.clone(), arg.preview_value()))
                .collect();
            render(&template, tr!("⟨剪贴板内容⟩", "⟨Clipboard text⟩"), &values)
        });
        self.ui()
            .preview
            .setString(&NSString::from_str(result.as_deref().unwrap_or("")));
        if let Err(error) = result {
            self.report(&error);
        }
    }
}

pub fn clipboard() -> String {
    NSPasteboard::generalPasteboard()
        .stringForType(unsafe { NSPasteboardTypeString })
        .map_or(String::new(), |s| s.to_string())
}

pub fn render(
    template: &Template,
    clipboard: &str,
    values: &HashMap<String, String>,
) -> Result<String, String> {
    render_with_preview(template, clipboard, values, false)
}

fn render_with_preview(
    template: &Template,
    clipboard: &str,
    values: &HashMap<String, String>,
    preview: bool,
) -> Result<String, String> {
    let now = NSDate::now();
    let date = |kind: &str, format: Option<&str>| {
        if kind == "timestamp" {
            return (now.timeIntervalSince1970().floor() as i64).to_string();
        }
        let formatter = NSDateFormatter::new();
        if let Some(format) = format {
            formatter.setDateFormat(Some(&NSString::from_str(format)));
        } else {
            formatter.setDateStyle(if kind == "time" {
                NSDateFormatterStyle::NoStyle
            } else {
                NSDateFormatterStyle::MediumStyle
            });
            formatter.setTimeStyle(if kind == "date" {
                NSDateFormatterStyle::NoStyle
            } else {
                NSDateFormatterStyle::ShortStyle
            });
        }
        formatter.stringFromDate(&now).to_string()
    };
    if preview {
        template.render_preview(clipboard, values, date)
    } else {
        template.render(clipboard, values, date)
    }
}

pub(crate) fn input(
    frame: objc2_foundation::NSRect,
    placeholder: &str,
    mtm: MainThreadMarker,
) -> Retained<NSTextField> {
    let field = NSTextField::initWithFrame(NSTextField::alloc(mtm), frame);
    field.setFont(Some(&NSFont::systemFontOfSize(14.0)));
    field.setPlaceholderString(Some(&NSString::from_str(placeholder)));
    field.setAccessibilityLabel(Some(&NSString::from_str(placeholder)));
    field
}

pub(crate) fn text_area(
    frame: objc2_foundation::NSRect,
    editable: bool,
    mtm: MainThreadMarker,
) -> (Retained<NSScrollView>, Retained<NSTextView>) {
    let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), frame);
    scroll.setBorderType(NSBorderType::BezelBorder);
    scroll.setHasVerticalScroller(true);
    scroll.setAutohidesScrollers(true);
    let text = NSTextView::initWithFrame(
        NSTextView::alloc(mtm),
        rect(0.0, 0.0, frame.size.width - 16.0, frame.size.height),
    );
    text.setRichText(false);
    text.setImportsGraphics(false);
    text.setEditable(editable);
    text.setSelectable(true);
    text.setFont(Some(&NSFont::systemFontOfSize(14.0)));
    text.setTextContainerInset(NSSize::new(10.0, 10.0));
    text.setVerticallyResizable(true);
    text.setHorizontallyResizable(false);
    text.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
    text.setMaxSize(NSSize::new(frame.size.width - 16.0, f64::MAX));
    if let Some(container) = unsafe { text.textContainer() } {
        container.setWidthTracksTextView(true);
        container.setContainerSize(NSSize::new(frame.size.width - 16.0, f64::MAX));
    }
    text.setAutomaticQuoteSubstitutionEnabled(false);
    text.setAutomaticDashSubstitutionEnabled(false);
    text.setAutomaticTextReplacementEnabled(false);
    text.setAutomaticSpellingCorrectionEnabled(false);
    text.setContinuousSpellCheckingEnabled(false);
    scroll.setDocumentView(Some(&text));
    (scroll, text)
}

enum ArgumentControl {
    Text(Retained<NSTextField>),
    Multiline(Retained<NSTextView>),
    Choice {
        popup: Retained<NSPopUpButton>,
        options: Vec<String>,
    },
}

impl ArgumentControl {
    fn value(&self) -> String {
        match self {
            Self::Text(field) => field.stringValue().to_string(),
            Self::Multiline(text) => text.string().to_string(),
            Self::Choice { popup, options } => popup
                .indexOfSelectedItem()
                .checked_sub(1)
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| options.get(index))
                .cloned()
                .unwrap_or_default(),
        }
    }
    fn view(&self) -> &NSView {
        match self {
            Self::Text(field) => field,
            Self::Multiline(text) => text,
            Self::Choice { popup, .. } => popup,
        }
    }
}

type Paste = Box<dyn Fn(String) -> Result<(), String>>;
pub(crate) struct ArgumentsState {
    window: OnceCell<Retained<NSWindow>>,
    fields: RefCell<Vec<ArgumentControl>>,
    preview: OnceCell<Retained<NSTextView>>,
    message: OnceCell<Retained<NSTextField>>,
    confirm: OnceCell<Retained<NSButton>>,
    template: Template,
    clipboard: String,
    paste: Paste,
}

define_class!(
    // SAFETY: This controller and its native input fields live on the main thread.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = ArgumentsState]
    pub(crate) struct SnippetArguments;
    unsafe impl NSObjectProtocol for SnippetArguments {}
    unsafe impl NSTextFieldDelegate for SnippetArguments {}
    unsafe impl NSControlTextEditingDelegate for SnippetArguments {
        #[unsafe(method(controlTextDidChange:))]
        fn changed(&self, _: &NSNotification) { self.update(); }
    }
    unsafe impl NSTextViewDelegate for SnippetArguments {}
    unsafe impl NSTextDelegate for SnippetArguments {
        #[unsafe(method(textDidChange:))]
        fn multiline_changed(&self, _: &NSNotification) { self.update(); }
    }
    impl SnippetArguments {
        #[unsafe(method(snippetArgumentChanged:))]
        fn choice_changed(&self, _: Option<&AnyObject>) { self.update(); }
        #[unsafe(method(pasteSnippet:))]
        fn paste(&self, _: Option<&AnyObject>) {
            match self.value().and_then(|value| (self.ivars().paste)(value)) {
                Ok(()) => self.window().close(),
                Err(error) => self.ivars().message.get().unwrap().setStringValue(&NSString::from_str(&error)),
            }
        }
        #[unsafe(method(cancelSnippet:))]
        fn cancel(&self, _: Option<&AnyObject>) { self.window().close(); }
    }
);
impl SnippetArguments {
    pub fn new(
        name: &str,
        template: Template,
        clipboard: String,
        paste: Paste,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let multiline = template
            .arguments
            .iter()
            .any(|arg| matches!(arg.kind, ArgumentKind::Multiline));
        let content_height: f64 = template
            .arguments
            .iter()
            .map(|arg| {
                if matches!(arg.kind, ArgumentKind::Multiline) {
                    88.0
                } else {
                    40.0
                }
            })
            .sum();
        let visible_height = content_height.min(300.0);
        let this = Self::alloc(mtm).set_ivars(ArgumentsState {
            window: OnceCell::new(),
            fields: RefCell::default(),
            preview: OnceCell::new(),
            message: OnceCell::new(),
            confirm: OnceCell::new(),
            template,
            clipboard,
            paste,
        });
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        let height = 300.0 + visible_height;
        let window = preferences_window(rect(0.0, 0.0, 580.0, height), mtm);
        window.setTitle(&NSString::from_str(name));
        let root = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 580.0, height));
        window.setContentView(Some(&root));
        root.addSubview(&label(
            tr!("填写片段", "Fill Snippet"),
            21.0,
            rect(24.0, height - 52.0, 530.0, 30.0),
            mtm,
        ));
        let fields_scroll = NSScrollView::initWithFrame(
            NSScrollView::alloc(mtm),
            rect(18.0, 236.0, 544.0, visible_height),
        );
        fields_scroll.setHasVerticalScroller(true);
        fields_scroll.setAutohidesScrollers(true);
        fields_scroll.setDrawsBackground(false);
        let fields_view =
            NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 528.0, content_height));
        let mut top = content_height;
        for arg in &this.ivars().template.arguments {
            let row_height = if matches!(arg.kind, ArgumentKind::Multiline) {
                88.0
            } else {
                40.0
            };
            let caption = if arg.required {
                format!("{} *", arg.name)
            } else {
                arg.name.clone()
            };
            fields_view.addSubview(&label(
                &caption,
                13.0,
                rect(6.0, top - 28.0, 140.0, 24.0),
                mtm,
            ));
            let control = match &arg.kind {
                ArgumentKind::Text => {
                    let field = input(rect(152.0, top - 30.0, 370.0, 28.0), &arg.name, mtm);
                    field.setStringValue(&NSString::from_str(arg.default.as_deref().unwrap_or("")));
                    unsafe {
                        field.setDelegate(Some(ProtocolObject::from_ref(&*this)));
                    }
                    fields_view.addSubview(&field);
                    ArgumentControl::Text(field)
                }
                ArgumentKind::Multiline => {
                    let (scroll, text) = text_area(rect(152.0, top - 80.0, 370.0, 76.0), true, mtm);
                    text.setString(&NSString::from_str(arg.default.as_deref().unwrap_or("")));
                    text.setAccessibilityLabel(Some(&NSString::from_str(&arg.name)));
                    text.setDelegate(Some(ProtocolObject::from_ref(&*this)));
                    fields_view.addSubview(&scroll);
                    ArgumentControl::Multiline(text)
                }
                ArgumentKind::Choice(options) => {
                    let popup = NSPopUpButton::initWithFrame_pullsDown(
                        NSPopUpButton::alloc(mtm),
                        rect(152.0, top - 30.0, 370.0, 28.0),
                        false,
                    );
                    // Prefix item titles so an option identical to the prompt remains distinct.
                    popup.addItemWithTitle(&NSString::from_str(tr!("请选择…", "Choose…")));
                    for (index, option) in options.iter().enumerate() {
                        popup.addItemWithTitle(&NSString::from_str(&format!(
                            "{}. {}",
                            index + 1,
                            option
                        )));
                    }
                    let selected = arg
                        .default
                        .as_ref()
                        .and_then(|value| options.iter().position(|option| option == value))
                        .map_or(0, |index| index + 1);
                    popup.selectItemAtIndex(selected as isize);
                    popup.setAccessibilityLabel(Some(&NSString::from_str(&arg.name)));
                    unsafe {
                        popup.setTarget(Some(&this));
                        popup.setAction(Some(sel!(snippetArgumentChanged:)));
                    }
                    fields_view.addSubview(&popup);
                    ArgumentControl::Choice {
                        popup,
                        options: options.clone(),
                    }
                }
            };
            this.ivars().fields.borrow_mut().push(control);
            top -= row_height;
        }
        fields_scroll.setDocumentView(Some(&fields_view));
        fields_scroll
            .contentView()
            .scrollToPoint(objc2_foundation::NSPoint::new(
                0.0,
                content_height - visible_height,
            ));
        root.addSubview(&fields_scroll);
        let (scroll, preview) = text_area(rect(24.0, 88.0, 532.0, 135.0), false, mtm);
        root.addSubview(&scroll);
        let message = hint("", rect(24.0, 50.0, 350.0, 32.0), mtm);
        message.setTextColor(Some(&NSColor::systemRedColor()));
        root.addSubview(&message);
        let confirm = button(
            tr!("粘贴", "Paste"),
            &this,
            sel!(pasteSnippet:),
            rect(450.0, 18.0, 106.0, 30.0),
            mtm,
        );
        confirm.setKeyEquivalent(ns_string!("\r"));
        if multiline {
            confirm.setKeyEquivalentModifierMask(NSEventModifierFlags::Command);
            root.addSubview(&hint(
                tr!("⌘↩ 粘贴 · ↩ 换行", "⌘↩ paste · ↩ new line"),
                rect(24.0, 20.0, 290.0, 24.0),
                mtm,
            ));
        }
        root.addSubview(&confirm);
        root.addSubview(&button(
            tr!("取消", "Cancel"),
            &this,
            sel!(cancelSnippet:),
            rect(334.0, 18.0, 106.0, 30.0),
            mtm,
        ));
        this.ivars().window.set(window).ok().unwrap();
        this.ivars().preview.set(preview).ok().unwrap();
        this.ivars().message.set(message).ok().unwrap();
        this.ivars().confirm.set(confirm).ok().unwrap();
        this.update();
        this
    }
    pub fn window(&self) -> &NSWindow {
        self.ivars().window.get().unwrap()
    }
    pub fn show(&self) {
        self.window().center();
        self.window().makeKeyAndOrderFront(None);
        if let Some(field) = self.ivars().fields.borrow().first() {
            self.window().makeFirstResponder(Some(field.view()));
        }
    }
    fn values(&self) -> HashMap<String, String> {
        self.ivars()
            .template
            .arguments
            .iter()
            .zip(self.ivars().fields.borrow().iter())
            .map(|(arg, field)| (arg.name.clone(), field.value()))
            .collect()
    }
    fn value(&self) -> Result<String, String> {
        render(
            &self.ivars().template,
            &self.ivars().clipboard,
            &self.values(),
        )
    }
    fn update(&self) {
        let result = self.value();
        self.ivars()
            .confirm
            .get()
            .unwrap()
            .setEnabled(result.is_ok());
        self.ivars()
            .message
            .get()
            .unwrap()
            .setStringValue(&NSString::from_str(
                result.as_ref().err().map_or("", String::as_str),
            ));
        let preview = render_with_preview(
            &self.ivars().template,
            &self.ivars().clipboard,
            &self.values(),
            true,
        )
        .unwrap_or_default();
        self.ivars()
            .preview
            .get()
            .unwrap()
            .setString(&NSString::from_str(&preview));
    }
}

impl Drop for EditorState {
    fn drop(&mut self) {
        if let Some(ui) = self.ui.get() {
            unsafe {
                ui.name.setDelegate(None);
            }
            ui.body.setDelegate(None);
            ui.root.removeFromSuperview();
        }
    }
}
impl Drop for ArgumentsState {
    fn drop(&mut self) {
        for field in self.fields.borrow().iter() {
            match field {
                ArgumentControl::Text(field) => unsafe {
                    field.setDelegate(None);
                },
                ArgumentControl::Multiline(text) => text.setDelegate(None),
                ArgumentControl::Choice { popup, .. } => unsafe {
                    popup.setTarget(None);
                },
            }
        }
        if let Some(window) = self.window.get() {
            window.close();
        }
    }
}
