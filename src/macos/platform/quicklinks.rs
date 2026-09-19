use objc2::rc::Retained;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSString, NSURL};
use std::collections::HashMap;
use winlane::features::quicklinks::{self, Template};
use winlane::tr;

pub fn render(
    template: &Template,
    clipboard: &str,
    values: &HashMap<String, String>,
) -> Result<String, String> {
    template.render(clipboard, values, |kind, format| {
        let body = if let Some(format) = format {
            format!(
                "{{{kind} format={}}}",
                serde_json::to_string(format).unwrap()
            )
        } else {
            format!("{{{kind}}}")
        };
        winlane::features::snippets::Template::parse(&body)
            .and_then(|template| {
                crate::macos::platform::template_context::render(&template, "", &HashMap::new())
            })
            .unwrap_or_default()
    })
}

pub fn destination_url(link: &str) -> Result<Retained<NSURL>, String> {
    let destination = quicklinks::destination(link)?;
    let url = if destination.starts_with('/') || destination.starts_with("~/") {
        let path = NSString::from_str(&destination).stringByExpandingTildeInPath();
        NSURL::fileURLWithPath(&path)
    } else {
        NSURL::URLWithString(&NSString::from_str(&destination))
            .ok_or_else(|| tr!("链接无效。", "Invalid URL.").to_owned())?
    };
    Ok(url)
}

pub fn open(link: &str, application: &str) -> Result<(), String> {
    let url = destination_url(link)?;
    if application.trim().is_empty() {
        if NSWorkspace::sharedWorkspace().openURL(&url) {
            Ok(())
        } else {
            Err(tr!(
                "无法打开链接，请检查地址及默认应用。",
                "Could not open the link. Check its address and default app."
            )
            .into())
        }
    } else {
        let target = application.trim();
        let mut command = std::process::Command::new("/usr/bin/open");
        command
            .arg(
                if target.contains('.')
                    && !target.contains('/')
                    && !target.ends_with(".app")
                    && !target.contains(' ')
                {
                    "-b"
                } else {
                    "-a"
                },
            )
            .arg(target)
            .arg("--")
            .arg(
                url.absoluteString()
                    .ok_or_else(|| tr!("链接无效。", "Invalid URL.").to_owned())?
                    .to_string(),
            );
        let result = command.output().map_err(|_| {
            tr!(
                "无法启动指定应用。",
                "Could not launch the selected application."
            )
            .to_owned()
        })?;
        if result.status.success() {
            Ok(())
        } else {
            Err(tr!(
                "找不到指定应用或无法打开链接，请检查“打开方式”。",
                "The selected app could not open this link. Check Open with."
            )
            .into())
        }
    }
}
