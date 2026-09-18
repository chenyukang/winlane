use crate::settings::{button, checkbox, hint, label, rect, set_action};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadOnly, sel};
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSString};
use winlane::clipboard::ClipboardSettings;
use winlane::tr;

pub struct ClipboardControls {
    enabled: Retained<NSButton>,
    persistent: Retained<NSButton>,
    count: Retained<NSTextField>,
    days: Retained<NSTextField>,
}
impl ClipboardControls {
    pub fn new(view: &NSView, target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let enabled = checkbox(tr!("记录剪贴板历史", "Record clipboard history"), mtm);
        enabled.setFrame(rect(30.0, 294.0, 620.0, 28.0));
        let persistent = checkbox(
            tr!(
                "保存在本机，重启后保留",
                "Save on this Mac and keep after restarting"
            ),
            mtm,
        );
        persistent.setFrame(rect(30.0, 249.0, 620.0, 28.0));
        for field in [&enabled, &persistent] {
            set_action(field, target, sel!(settingsChanged:));
            view.addSubview(field);
        }
        let count =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(440.0, 199.0, 170.0, 28.0));
        let days =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(440.0, 153.0, 170.0, 28.0));
        for (title, y, field) in [
            (
                tr!("最多保留条数（1–1000）", "Maximum entries (1–1000)"),
                201.0,
                &count,
            ),
            (
                tr!("保留天数（1–365）", "Retention in days (1–365)"),
                155.0,
                &days,
            ),
        ] {
            view.addSubview(&label(title, 14.0, rect(30.0, y, 400.0, 26.0), mtm));
            view.addSubview(field);
            field.cell().unwrap().setSendsActionOnEndEditing(true);
            set_action(field, target, sel!(settingsChanged:));
        }
        let description = hint(
            tr!(
                "搜索 clipboard 打开历史；关闭记录会暂停采集，已有历史仍可使用。\n跳过密码标记和临时数据。支持文本、链接及 PNG/TIFF 图片。\n历史仅保存在本机；取消保存会删除持久副本，图片使用退出时清理的临时文件。",
                "Search clipboard to open history. Pausing keeps existing entries available.\nPassword markers and transient data are skipped. Text, links and PNG/TIFF images.\nLocal only. With saving off, images use temporary files removed on quit."
            ),
            rect(30.0, 65.0, 650.0, 72.0),
            mtm,
        );
        description.setMaximumNumberOfLines(0);
        view.addSubview(&description);
        view.addSubview(&button(
            tr!("清空剪贴板历史…", "Clear Clipboard History…"),
            target,
            sel!(clearClipboardHistory:),
            rect(30.0, 16.0, 260.0, 30.0),
            mtm,
        ));
        Self {
            enabled,
            persistent,
            count,
            days,
        }
    }
    pub fn fill(&self, settings: &ClipboardSettings) {
        self.enabled.setState(if settings.enabled {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        self.persistent.setState(if settings.persistent {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        self.count
            .setStringValue(&NSString::from_str(&settings.max_items.to_string()));
        self.days
            .setStringValue(&NSString::from_str(&settings.retention_days.to_string()));
    }
    pub fn read(&self) -> Result<ClipboardSettings, String> {
        let error = || {
            tr!(
                "历史条数和天数必须填写正整数。",
                "History count and days must be positive integers."
            )
            .to_string()
        };
        let settings = ClipboardSettings {
            enabled: self.enabled.state() == NSControlStateValueOn,
            persistent: self.persistent.state() == NSControlStateValueOn,
            max_items: self
                .count
                .stringValue()
                .to_string()
                .trim()
                .parse()
                .map_err(|_| error())?,
            retention_days: self
                .days
                .stringValue()
                .to_string()
                .trim()
                .parse()
                .map_err(|_| error())?,
        };
        settings.validate()?;
        Ok(settings)
    }
}
