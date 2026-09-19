use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
use objc2_foundation::{NSDate, NSDateFormatter, NSDateFormatterStyle, NSString};
use std::collections::HashMap;
use winlane::features::snippets::Template;

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

pub(crate) fn render_with_preview(
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
