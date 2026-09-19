use crate::macos::ui::controls::{button, label, rect};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{MainThreadOnly, sel};
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSRect, NSString};
use std::collections::HashMap;
use winlane::core::config::DisplayDensity;
use winlane::features::quicklinks::{Quicklink, Template};
use winlane::features::snippets::{Argument, ArgumentKind};
use winlane::tr;

pub struct Input {
    pub link: Quicklink,
    pub template: Template,
    pub clipboard: String,
    pub values: HashMap<String, String>,
    pub active: usize,
    pub error: Option<String>,
}
impl Input {
    pub fn new(link: Quicklink, template: Template, clipboard: String) -> Self {
        let values = template
            .arguments
            .iter()
            .map(|arg| (arg.name.clone(), arg.default.clone().unwrap_or_default()))
            .collect();
        Self {
            link,
            template,
            clipboard,
            values,
            active: 0,
            error: None,
        }
    }
    pub fn destination(&self) -> Result<String, String> {
        crate::macos::platform::quicklinks::render(&self.template, &self.clipboard, &self.values)
    }
    pub fn extra_height(&self, density: DisplayDensity) -> f64 {
        (self.template.arguments.len().saturating_sub(1) / 3) as f64 * row_height(density)
    }
}
fn row_height(density: DisplayDensity) -> f64 {
    match density {
        DisplayDensity::Compact => 38.0,
        DisplayDensity::Normal => 42.0,
    }
}

enum Field {
    Text(Retained<NSTextField>),
    Choice(Retained<NSPopUpButton>, Vec<String>),
}
impl Field {
    fn control(&self) -> &NSControl {
        match self {
            Self::Text(field) => field,
            Self::Choice(field, _) => field,
        }
    }
    fn value(&self) -> String {
        match self {
            Self::Text(field) => field.stringValue().to_string(),
            Self::Choice(field, options) => field
                .indexOfSelectedItem()
                .checked_sub(1)
                .and_then(|i| options.get(i as usize))
                .cloned()
                .unwrap_or_default(),
        }
    }
    fn fill(&self, value: &str) {
        match self {
            Self::Text(field) if field.stringValue().to_string() != value => {
                field.setStringValue(&NSString::from_str(value))
            }
            Self::Choice(field, options) => field.selectItemAtIndex(
                options.iter().position(|v| v == value).map_or(0, |i| i + 1) as isize,
            ),
            _ => {}
        }
    }
}

pub struct Bar {
    pub view: Retained<NSView>,
    title: Retained<NSTextField>,
    back: Retained<NSButton>,
    fields: Vec<Field>,
    arguments: Vec<Argument>,
}
impl Bar {
    pub fn new(target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 700.0, 64.0));
        let back = button(
            "‹",
            target,
            sel!(leaveScopedSearch:),
            rect(8.0, 12.0, 28.0, 34.0),
            mtm,
        );
        back.setBordered(false);
        back.setAccessibilityLabel(Some(&NSString::from_str(tr!("返回搜索", "Back to search"))));
        view.addSubview(&back);
        let title = label("", 14.0, rect(42.0, 16.0, 166.0, 26.0), mtm);
        title.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        title.setMaximumNumberOfLines(1);
        view.addSubview(&title);
        view.setHidden(true);
        Self {
            view,
            title,
            back,
            fields: Vec::new(),
            arguments: Vec::new(),
        }
    }
    pub fn control(&self, index: usize) -> Option<&NSControl> {
        self.fields.get(index).map(Field::control)
    }
    pub fn index(&self, control: &NSControl) -> Option<usize> {
        self.fields
            .iter()
            .position(|field| std::ptr::eq(field.control(), control))
    }
    pub fn value(&self, index: usize) -> Option<String> {
        self.fields.get(index).map(Field::value)
    }
    fn clear_fields(&mut self) {
        for field in self.fields.drain(..) {
            match &field {
                // SAFETY: Clear AppKit's weak delegate/target before releasing the field.
                Field::Text(field) => unsafe {
                    field.setDelegate(None);
                },
                Field::Choice(field, _) => unsafe {
                    field.setTarget(None);
                },
            }
            field.control().removeFromSuperview();
        }
        self.arguments.clear();
    }
    pub fn clear(&mut self) {
        self.view.setHidden(true);
        self.clear_fields();
        self.title.setStringValue(&NSString::from_str(""));
    }
    pub fn render(
        &mut self,
        input: &Input,
        frame: NSRect,
        density: DisplayDensity,
        target: &ProtocolObject<dyn NSTextFieldDelegate>,
        mtm: MainThreadMarker,
    ) {
        if self.arguments != input.template.arguments {
            self.clear_fields();
            for arg in &input.template.arguments {
                let field = match &arg.kind {
                    ArgumentKind::Choice(options) => {
                        let field = NSPopUpButton::initWithFrame_pullsDown(
                            NSPopUpButton::alloc(mtm),
                            NSRect::ZERO,
                            false,
                        );
                        field.addItemWithTitle(&NSString::from_str(&arg.name));
                        for (i, option) in options.iter().enumerate() {
                            field.addItemWithTitle(&NSString::from_str(&format!(
                                "{}. {}",
                                i + 1,
                                option
                            )));
                        }
                        unsafe {
                            field.setTarget(Some(target.as_ref()));
                            field.setAction(Some(sel!(quicklinkArgumentChanged:)));
                        }
                        Field::Choice(field, options.clone())
                    }
                    _ => {
                        let field =
                            NSTextField::initWithFrame(NSTextField::alloc(mtm), NSRect::ZERO);
                        field.setBezelStyle(NSTextFieldBezelStyle::RoundedBezel);
                        field.setFocusRingType(NSFocusRingType::None);
                        field.setPlaceholderString(Some(&NSString::from_str(&arg.name)));
                        field.setAccessibilityLabel(Some(&NSString::from_str(&arg.name)));
                        field.setUsesSingleLineMode(true);
                        unsafe {
                            field.setDelegate(Some(target));
                        }
                        Field::Text(field)
                    }
                };
                self.view.addSubview(field.control());
                self.fields.push(field);
            }
            self.arguments.clone_from(&input.template.arguments);
        }
        self.view.setFrame(frame);
        self.view.setHidden(false);
        self.title
            .setStringValue(&NSString::from_str(&input.link.name));
        self.title
            .setToolTip(Some(&NSString::from_str(&input.link.name)));
        let row = row_height(density);
        let top = frame.size.height - 35.0;
        self.back.setFrame(rect(8.0, top - 17.0, 28.0, 34.0));
        self.title.setFrame(rect(42.0, top - 13.0, 166.0, 26.0));
        let columns = self.fields.len().clamp(1, 3);
        let width = (frame.size.width - 238.0 - (columns - 1) as f64 * 8.0) / columns as f64;
        for (i, field) in self.fields.iter().enumerate() {
            field.control().setFrame(rect(
                222.0 + (i % columns) as f64 * (width + 8.0),
                top - 16.0 - (i / columns) as f64 * row,
                width,
                32.0,
            ));
            field.control().setFont(Some(&NSFont::systemFontOfSize(
                if density == DisplayDensity::Normal {
                    16.0
                } else {
                    15.0
                },
            )));
            field.fill(
                input
                    .values
                    .get(&self.arguments[i].name)
                    .map_or("", String::as_str),
            );
        }
    }
}
impl Drop for Bar {
    fn drop(&mut self) {
        self.clear_fields();
    }
}
