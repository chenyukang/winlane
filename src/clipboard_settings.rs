use crate::settings::{
    button, checkbox, hint, label, rect, row_divider, set_action, settings_group,
};
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
        let recording = settings_group(view, tr!("记录", "Recording"), 570.0, 156.0, mtm);
        let enabled = checkbox(tr!("记录剪贴板历史", "Record clipboard history"), mtm);
        enabled.setFrame(rect(20.0, 114.0, 696.0, 28.0));
        let persistent = checkbox(
            tr!(
                "保存在本机，重启后保留",
                "Save on this Mac and keep after restarting"
            ),
            mtm,
        );
        persistent.setFrame(rect(20.0, 70.0, 696.0, 28.0));
        for field in [&enabled, &persistent] {
            set_action(field, target, sel!(settingsChanged:));
            recording.addSubview(field);
        }
        recording.addSubview(&hint(tr!("支持文本、链接和图片；搜索 clipboard 打开历史。暂停记录不会清空已有内容。", "Text, links, and images. Search clipboard to browse history. Pausing keeps existing entries."), rect(22.0, 16.0, 696.0, 42.0), mtm));
        let retention = settings_group(view, tr!("保留策略", "Retention"), 360.0, 140.0, mtm);
        let count =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(550.0, 90.0, 170.0, 28.0));
        let days =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(550.0, 21.0, 170.0, 28.0));
        for (title, y, field) in [
            (
                tr!("最多保留条数（1–1000）", "Maximum entries (1–1000)"),
                92.0,
                &count,
            ),
            (
                tr!("保留天数（1–365）", "Retention in days (1–365)"),
                23.0,
                &days,
            ),
        ] {
            retention.addSubview(&label(title, 14.0, rect(20.0, y, 500.0, 26.0), mtm));
            retention.addSubview(field);
            field.setAccessibilityLabel(Some(&NSString::from_str(title)));
            field.cell().unwrap().setSendsActionOnEndEditing(true);
            set_action(field, target, sel!(settingsChanged:));
        }
        row_divider(&retention, 70.0, mtm);
        let clearing = settings_group(view, tr!("清理历史", "Clear history"), 166.0, 80.0, mtm);
        clearing.addSubview(&hint(
            tr!(
                "删除全部已保存的文本和图片。",
                "Remove all saved text and images."
            ),
            rect(20.0, 23.0, 420.0, 34.0),
            mtm,
        ));
        clearing.addSubview(&button(
            tr!("清空剪贴板历史…", "Clear Clipboard History…"),
            target,
            sel!(clearClipboardHistory:),
            rect(466.0, 24.0, 254.0, 30.0),
            mtm,
        ));
        let privacy = hint(
            tr!(
                "跳过密码标记和临时数据。关闭本机保存会删除持久副本；临时图片在退出时清理。",
                "Password markers and transient data are skipped. Turning off saving removes stored copies; temporary images are cleared on quit."
            ),
            rect(6.0, 0.0, 728.0, 42.0),
            mtm,
        );
        privacy.setMaximumNumberOfLines(3);
        view.addSubview(&privacy);
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
