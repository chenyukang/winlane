use super::*;

pub(super) struct FilesPage {
    roots: Retained<NSTextField>,
    excluded: Retained<NSTextField>,
    generated: Retained<NSButton>,
}

impl FilesPage {
    pub(super) fn new(host: &NSView, target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let scope = settings_group(host, tr!("搜索范围", "Search folders"), 570.0, 130.0, mtm);
        let make_field = |parent: &NSView| {
            let field =
                NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(20.0, 55.0, 696.0, 30.0));
            field.setFont(Some(&NSFont::systemFontOfSize(13.0)));
            field.cell().unwrap().setSendsActionOnEndEditing(true);
            set_action(&field, target, sel!(settingsChanged:));
            parent.addSubview(&field);
            field
        };
        scope.addSubview(&label(
            tr!("包含这些目录", "Include these folders"),
            14.0,
            rect(20.0, 92.0, 696.0, 24.0),
            mtm,
        ));
        let roots = make_field(&scope);
        scope.addSubview(&hint(tr!("~ 代表主目录。多个路径用分号分隔，例如 ~/Documents; ~/Downloads", "~ is your home folder. Separate paths with semicolons, e.g. ~/Documents; ~/Downloads"), rect(20.0, 12.0, 696.0, 36.0), mtm));
        let exclusions = settings_group(host, tr!("排除", "Exclusions"), 390.0, 168.0, mtm);
        exclusions.addSubview(&label(
            tr!("跳过这些目录", "Skip these folders"),
            14.0,
            rect(20.0, 130.0, 696.0, 24.0),
            mtm,
        ));
        let excluded = make_field(&exclusions);
        excluded.setFrameOrigin(NSPoint::new(20.0, 91.0));
        let generated = checkbox(
            tr!("隐藏构建和依赖目录", "Hide build and dependency folders"),
            mtm,
        );
        generated.setFrame(rect(20.0, 48.0, 690.0, 28.0));
        set_action(&generated, target, sel!(settingsChanged:));
        exclusions.addSubview(&generated);
        exclusions.addSubview(&hint(
            tr!(
                "隐藏文件和应用包默认不参与关键词搜索。",
                "Hidden files and app bundles are excluded from keyword search."
            ),
            rect(20.0, 10.0, 690.0, 30.0),
            mtm,
        ));
        let recent = settings_group(host, tr!("最近打开", "Recently opened"), 172.0, 80.0, mtm);
        recent.addSubview(&hint(
            tr!(
                "仅在本机保存通过 Winlane 打开的最近 25 个文件路径。",
                "Keep the last 25 paths opened through Winlane on this Mac."
            ),
            rect(20.0, 18.0, 492.0, 44.0),
            mtm,
        ));
        recent.addSubview(&button(
            tr!("清除记录", "Clear Recents"),
            target,
            sel!(clearRecentFiles:),
            rect(540.0, 26.0, 174.0, 30.0),
            mtm,
        ));
        host.addSubview(&hint(tr!("关键词搜索使用 Spotlight 索引。直接输入绝对路径可浏览其他目录，不受以上范围限制。", "Keyword search uses Spotlight. Type an absolute path to browse other folders, regardless of the search scope above."), rect(16.0, 4.0, 708.0, 50.0), mtm));
        Self {
            roots,
            excluded,
            generated,
        }
    }
    pub(super) fn fill(&self, config: &Config) {
        self.roots
            .setStringValue(&NSString::from_str(&config.files.roots.join("; ")));
        self.excluded
            .setStringValue(&NSString::from_str(&config.files.excluded.join("; ")));
        self.generated.setState(if config.files.hide_generated {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
    }
    pub(super) fn read(&self, config: &mut Config) -> Result<(), String> {
        let paths = |field: &NSTextField| {
            field
                .stringValue()
                .to_string()
                .split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        };
        config.files = winlane::features::files::Settings {
            roots: paths(&self.roots),
            excluded: paths(&self.excluded),
            hide_generated: self.generated.state() == NSControlStateValueOn,
        };
        config.files.validate()
    }
}

#[cfg(test)]
#[path = "../../../../tests/native/settings_files.rs"]
pub(crate) mod tests;
