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
    if preview {
        template.render_preview(clipboard, values, date_value)
    } else {
        template.render(clipboard, values, date_value)
    }
}

/// The rendered text and how many characters at its end come after a
/// `{cursor}` marker, so the paste can move the caret there.
pub fn render_with_caret(
    template: &Template,
    clipboard: &str,
    values: &HashMap<String, String>,
) -> Result<(String, usize), String> {
    let (text, trailing) = template.render_with_caret(clipboard, values, date_value)?;
    Ok((text, trailing.unwrap_or(0)))
}

fn date_value(kind: &str, format: Option<&str>) -> String {
    let now = NSDate::now();
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
}
