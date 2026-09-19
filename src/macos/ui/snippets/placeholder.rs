use crate::macos::platform::template_context::render;
use crate::macos::ui::controls::{button, hint, label, rect};
use crate::macos::ui::controls::{input, text_area};
use crate::macos::ui::snippets::SnippetEditor;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{MainThreadOnly, sel};
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSString, ns_string};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use winlane::features::snippets::{
    Argument, ArgumentKind, DATE_PRESETS, Template, date_placeholder,
};
use winlane::{tr, trf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BuilderMode {
    Date,
    Field,
}

pub struct PlaceholderBuilder {
    pub view: Retained<NSView>,
    mode: Cell<BuilderMode>,
    title: Retained<NSTextField>,
    preset: Retained<NSPopUpButton>,
    format: Retained<NSTextField>,
    format_label: Retained<NSTextField>,
    date_widgets: Vec<Retained<NSView>>,
    field_widgets: Vec<Retained<NSView>>,
    reuse: Retained<NSPopUpButton>,
    existing: RefCell<Vec<Argument>>,
    name: Retained<NSTextField>,
    kind: Retained<NSPopUpButton>,
    default: Retained<NSTextView>,
    required: Retained<NSButton>,
    options: Retained<NSTextView>,
    options_scroll: Retained<NSScrollView>,
    options_label: Retained<NSTextField>,
    preview: Retained<NSTextField>,
    error: Retained<NSTextField>,
    insert: Retained<NSButton>,
}

impl PlaceholderBuilder {
    pub fn new(target: &SnippetEditor, mtm: MainThreadMarker) -> Self {
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(212.0, 42.0, 436.0, 440.0));
        view.setHidden(true);
        let title = label("", 17.0, rect(0.0, 410.0, 206.0, 26.0), mtm);
        view.addSubview(&title);
        let preset_label = label(
            tr!("日期格式", "Date format"),
            13.0,
            rect(0.0, 372.0, 105.0, 24.0),
            mtm,
        );
        let preset = popup(
            &[],
            target,
            sel!(placeholderBuilderChanged:),
            rect(112.0, 368.0, 320.0, 30.0),
            mtm,
        );
        let format_label = label(
            tr!("自定义格式", "Custom format"),
            13.0,
            rect(0.0, 332.0, 108.0, 24.0),
            mtm,
        );
        let format = input(rect(112.0, 328.0, 320.0, 28.0), "yyyy-MM-dd", mtm);
        let date_help = hint(
            tr!(
                "yyyy 年 · MM 月 · dd 日 · HH 时 · mm 分\n例如 yyyy-MM-dd HH:mm，日期在粘贴时更新。",
                "yyyy year · MM month · dd day · HH hour · mm minute\nFor example, yyyy-MM-dd HH:mm. Dates update when pasted."
            ),
            rect(0.0, 264.0, 432.0, 48.0),
            mtm,
        );
        let date_widgets: Vec<Retained<NSView>> = vec![
            preset_label.into_super().into_super(),
            preset.clone().into_super().into_super().into_super(),
            format_label.clone().into_super().into_super(),
            format.clone().into_super().into_super(),
            date_help.into_super().into_super(),
        ];
        for widget in &date_widgets {
            view.addSubview(widget);
        }

        let reuse = popup(
            &[],
            target,
            sel!(reuseSnippetField:),
            rect(208.0, 408.0, 224.0, 28.0),
            mtm,
        );
        let name_label = label(
            tr!("字段名称", "Field name"),
            13.0,
            rect(0.0, 372.0, 104.0, 24.0),
            mtm,
        );
        let name = input(
            rect(112.0, 368.0, 320.0, 28.0),
            tr!("例如：姓名", "For example: Name"),
            mtm,
        );
        let kind_label = label(
            tr!("类型", "Type"),
            13.0,
            rect(0.0, 332.0, 104.0, 24.0),
            mtm,
        );
        let kind = popup(
            &[
                tr!("单行文本", "Text"),
                tr!("多行文本", "Multiline"),
                tr!("下拉选项", "Dropdown"),
            ],
            target,
            sel!(placeholderBuilderChanged:),
            rect(112.0, 328.0, 320.0, 30.0),
            mtm,
        );
        let default_label = label(
            tr!("默认值（可选）", "Default value (optional)"),
            13.0,
            rect(0.0, 294.0, 432.0, 22.0),
            mtm,
        );
        let (default_scroll, default) = text_area(rect(0.0, 239.0, 432.0, 50.0), true, mtm);
        default.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "字段默认值",
            "Field default value"
        ))));
        let required = button(
            tr!("必填", "Required"),
            target,
            sel!(placeholderBuilderChanged:),
            rect(0.0, 205.0, 190.0, 26.0),
            mtm,
        );
        required.setButtonType(NSButtonType::Switch);
        let options_label = label(
            tr!("选项（每行一个）", "Options (one per line)"),
            13.0,
            rect(0.0, 178.0, 432.0, 22.0),
            mtm,
        );
        let (options_scroll, options) = text_area(rect(0.0, 103.0, 432.0, 72.0), true, mtm);
        options.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "下拉选项",
            "Dropdown options"
        ))));
        let field_widgets: Vec<Retained<NSView>> = vec![
            reuse.clone().into_super().into_super().into_super(),
            name_label.into_super().into_super(),
            name.clone().into_super().into_super(),
            kind_label.into_super().into_super(),
            kind.clone().into_super().into_super().into_super(),
            default_label.into_super().into_super(),
            default_scroll.into_super(),
            required.clone().into_super().into_super(),
            options_label.clone().into_super().into_super(),
            options_scroll.clone().into_super(),
        ];
        for widget in &field_widgets {
            view.addSubview(widget);
        }
        for field in [&name, &format] {
            unsafe {
                field.setDelegate(Some(ProtocolObject::from_ref(target)));
            }
        }
        for text in [&default, &options] {
            text.setDelegate(Some(ProtocolObject::from_ref(target)));
        }
        let preview = label("", 14.0, rect(0.0, 66.0, 432.0, 34.0), mtm);
        preview.setMaximumNumberOfLines(2);
        view.addSubview(&preview);
        let error = hint("", rect(0.0, 33.0, 432.0, 32.0), mtm);
        error.setTextColor(Some(&NSColor::systemRedColor()));
        view.addSubview(&error);
        let insert = button(
            tr!("插入", "Insert"),
            target,
            sel!(commitSnippetPlaceholder:),
            rect(332.0, 0.0, 100.0, 28.0),
            mtm,
        );
        insert.setKeyEquivalent(ns_string!("\r"));
        insert.setKeyEquivalentModifierMask(NSEventModifierFlags::Command);
        view.addSubview(&insert);
        view.addSubview(&button(
            tr!("取消", "Cancel"),
            target,
            sel!(cancelSnippetPlaceholder:),
            rect(224.0, 0.0, 100.0, 28.0),
            mtm,
        ));
        Self {
            view,
            mode: Cell::new(BuilderMode::Date),
            title,
            preset,
            format,
            format_label,
            date_widgets,
            field_widgets,
            reuse,
            existing: RefCell::default(),
            name,
            kind,
            default,
            required,
            options,
            options_scroll,
            options_label,
            preview,
            error,
            insert,
        }
    }

    pub fn begin(&self, mode: BuilderMode, arguments: &[Argument]) {
        self.mode.set(mode);
        self.title.setStringValue(&NSString::from_str(match mode {
            BuilderMode::Date => tr!("插入日期与时间", "Insert Date & Time"),
            BuilderMode::Field => tr!("插入输入字段", "Insert Input Field"),
        }));
        for view in &self.date_widgets {
            view.setHidden(mode != BuilderMode::Date);
        }
        for view in &self.field_widgets {
            view.setHidden(mode != BuilderMode::Field);
        }
        if mode == BuilderMode::Date {
            self.preset.removeAllItems();
            for preset in DATE_PRESETS {
                let example = render(&Template::parse(preset.token).unwrap(), "", &HashMap::new())
                    .unwrap_or_default();
                self.preset.addItemWithTitle(&NSString::from_str(&format!(
                    "{} · {}",
                    match winlane::core::i18n::locale() {
                        winlane::core::i18n::Locale::Chinese => preset.title_zh,
                        winlane::core::i18n::Locale::English => preset.title_en,
                    },
                    example
                )));
            }
            self.preset
                .addItemWithTitle(&NSString::from_str(tr!("自定义…", "Custom…")));
            self.preset.selectItemAtIndex(1);
            self.format.setStringValue(ns_string!("yyyy-MM-dd"));
        } else {
            self.existing.replace(arguments.to_vec());
            self.reuse.removeAllItems();
            self.reuse.addItemWithTitle(&NSString::from_str(tr!(
                "新字段／复用已有字段…",
                "New / reuse existing field…"
            )));
            for argument in arguments {
                self.reuse
                    .addItemWithTitle(&NSString::from_str(&argument.name));
            }
            self.name.setStringValue(ns_string!(""));
            self.kind.selectItemAtIndex(0);
            self.default.setString(ns_string!(""));
            self.options.setString(ns_string!(""));
            self.required.setState(NSControlStateValueOn);
        }
        self.view.setHidden(false);
        self.update();
        if let Some(window) = self.view.window() {
            let focus: &NSView = if mode == BuilderMode::Date {
                &self.preset
            } else {
                &self.name
            };
            window.makeFirstResponder(Some(focus));
        }
    }

    pub fn report(&self, error: &str) {
        self.error.setStringValue(&NSString::from_str(error));
    }

    pub fn reuse(&self) {
        let Some(index) = self
            .reuse
            .indexOfSelectedItem()
            .checked_sub(1)
            .filter(|index| *index >= 0)
        else {
            self.name.setStringValue(ns_string!(""));
            self.default.setString(ns_string!(""));
            self.options.setString(ns_string!(""));
            self.kind.selectItemAtIndex(0);
            self.required.setState(NSControlStateValueOn);
            self.update();
            return;
        };
        let args = self.existing.borrow();
        let Some(arg) = args.get(index as usize) else {
            return;
        };
        self.name.setStringValue(&NSString::from_str(&arg.name));
        self.default
            .setString(&NSString::from_str(arg.default.as_deref().unwrap_or("")));
        self.required.setState(if arg.required {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        self.kind.selectItemAtIndex(match arg.kind {
            ArgumentKind::Text => 0,
            ArgumentKind::Multiline => 1,
            ArgumentKind::Choice(_) => 2,
        });
        self.options.setString(&NSString::from_str(
            match &arg.kind {
                ArgumentKind::Choice(options) => options.join("\n"),
                _ => String::new(),
            }
            .as_str(),
        ));
        self.update();
    }

    pub fn token(&self) -> Result<String, String> {
        match self.mode.get() {
            BuilderMode::Date => {
                if let Some(preset) = DATE_PRESETS.get(self.preset.indexOfSelectedItem() as usize) {
                    Ok(preset.token.into())
                } else {
                    date_placeholder(&self.format.stringValue().to_string())
                }
            }
            BuilderMode::Field => self.argument().placeholder(),
        }
    }
    fn argument(&self) -> Argument {
        let default = self.default.string().to_string();
        Argument {
            name: self.name.stringValue().to_string().trim().into(),
            default: (!default.is_empty()).then_some(default),
            required: self.required.state() == NSControlStateValueOn,
            kind: match self.kind.indexOfSelectedItem() {
                1 => ArgumentKind::Multiline,
                2 => ArgumentKind::Choice(
                    self.options
                        .string()
                        .to_string()
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty())
                        .map(str::to_owned)
                        .collect(),
                ),
                _ => ArgumentKind::Text,
            },
        }
    }
    pub fn update(&self) {
        let custom_date = self.mode.get() == BuilderMode::Date
            && self.preset.indexOfSelectedItem() as usize == DATE_PRESETS.len();
        self.format.setHidden(!custom_date);
        self.format_label.setHidden(!custom_date);
        let choices = self.mode.get() == BuilderMode::Field && self.kind.indexOfSelectedItem() == 2;
        self.options_scroll.setHidden(!choices);
        self.options_label.setHidden(!choices);
        let result = self.token().and_then(|token| {
            let template = Template::parse(&token)?;
            let values = template
                .arguments
                .iter()
                .map(|arg| (arg.name.clone(), arg.preview_value()))
                .collect();
            render(&template, "", &values)
        });
        self.insert.setEnabled(result.is_ok());
        match result {
            Ok(preview) => {
                self.preview.setStringValue(&NSString::from_str(&trf!(
                    "预览：{}",
                    "Preview: {}",
                    preview
                )));
                self.error.setStringValue(ns_string!(""));
            }
            Err(error) => {
                self.preview.setStringValue(ns_string!(""));
                self.error.setStringValue(&NSString::from_str(&error));
            }
        }
    }
    pub fn handles(&self, object: &AnyObject) -> bool {
        !self.view.isHidden()
            && object
                .downcast_ref::<NSView>()
                .is_some_and(|view| view.isDescendantOf(&self.view))
    }
}
impl Drop for PlaceholderBuilder {
    fn drop(&mut self) {
        for field in [&self.name, &self.format] {
            unsafe {
                field.setDelegate(None);
            }
        }
        self.default.setDelegate(None);
        self.options.setDelegate(None);
    }
}
fn popup(
    titles: &[&str],
    target: &SnippetEditor,
    action: objc2::runtime::Sel,
    frame: objc2_foundation::NSRect,
    mtm: MainThreadMarker,
) -> Retained<NSPopUpButton> {
    let popup = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), frame, false);
    for title in titles {
        popup.addItemWithTitle(&NSString::from_str(title));
    }
    unsafe {
        popup.setTarget(Some(target));
        popup.setAction(Some(action));
    }
    popup
}

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../../tests/native/snippet_placeholder.rs"]
pub(crate) mod tests;
