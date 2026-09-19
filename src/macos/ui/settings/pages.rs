use super::*;

pub(super) struct GeneralPage {
    pub(super) language: Retained<NSPopUpButton>,
    pub(super) login: Retained<NSButton>,
    pub(super) login_status: Retained<NSTextField>,
    pub(super) automatic_updates: Retained<NSButton>,
    pub(super) check_updates: Retained<NSButton>,
    pub(super) update_status: Retained<NSTextField>,
}

impl GeneralPage {
    pub(super) fn new(general: &NSView, target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let localization = settings_group(general, tr!("语言", "Language"), 570.0, 80.0, mtm);
        row_text(
            &localization,
            tr!("界面语言", "Interface language"),
            tr!("更改立即生效。", "Changes take effect immediately."),
            80.0,
            390.0,
            mtm,
        );
        let language = popup(
            &[tr!("跟随系统", "System"), "中文", "English"],
            rect(420.0, 25.0, 300.0, 28.0),
            mtm,
        );
        localization.addSubview(&language);
        let startup = settings_group(general, tr!("启动", "Startup"), 436.0, 114.0, mtm);
        let login = checkbox(tr!("登录时自动启动", "Launch at login"), mtm);
        login.setFrame(rect(20.0, 72.0, 360.0, 26.0));
        set_action(&login, target, sel!(toggleLogin:));
        startup.addSubview(&login);
        startup.addSubview(&button(
            tr!("管理登录项…", "Manage Login Items…"),
            target,
            sel!(manageLogin:),
            rect(500.0, 69.0, 220.0, 30.0),
            mtm,
        ));
        let login_status = hint("", rect(22.0, 18.0, 696.0, 44.0), mtm);
        startup.addSubview(&login_status);
        let updates = settings_group(general, tr!("更新", "Updates"), 268.0, 116.0, mtm);
        let automatic_updates =
            checkbox(tr!("自动检查更新", "Automatically check for updates"), mtm);
        automatic_updates.setFrame(rect(20.0, 73.0, 448.0, 27.0));
        set_action(&automatic_updates, target, sel!(toggleAutomaticUpdates:));
        automatic_updates.setEnabled(false);
        updates.addSubview(&automatic_updates);
        let check_updates = button(
            tr!("检查更新…", "Check for Updates…"),
            target,
            sel!(checkForUpdates:),
            rect(500.0, 71.0, 220.0, 28.0),
            mtm,
        );
        check_updates.setEnabled(false);
        updates.addSubview(&check_updates);
        let update_status = hint("", rect(22.0, 14.0, 696.0, 48.0), mtm);
        update_status.setMaximumNumberOfLines(3);
        updates.addSubview(&update_status);
        general.addSubview(&hint(
            tr!(
                "设置保存在本机；窗口标题与搜索历史不会保存。",
                "Settings stay on this Mac. Window titles and search history are not saved."
            ),
            rect(16.0, 43.0, 708.0, 42.0),
            mtm,
        ));

        set_action(&language, target, sel!(settingsChanged:));
        Self {
            language,
            login,
            login_status,
            automatic_updates,
            check_updates,
            update_status,
        }
    }
    pub(super) fn fill(&self, config: &Config) {
        self.language.selectItemAtIndex(match config.language {
            Language::System => 0,
            Language::Chinese => 1,
            Language::English => 2,
        });
        self.update_login_status();
    }
    pub(super) fn update_updater(&self, available: bool, automatic: bool, error: Option<&str>) {
        self.automatic_updates.setEnabled(available);
        self.automatic_updates.setState(if automatic {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        self.check_updates.setEnabled(available || error.is_some());
        let status = if error.is_some() {
            tr!(
                "更新组件未能启动。点击“检查更新”查看详情。",
                "The updater could not start. Choose Check for Updates for details."
            )
        } else if available {
            tr!(
                "每天检查一次；确认后才会下载、安装并重启。",
                "Checks once a day. Downloads, installation, and relaunch require your confirmation."
            )
        } else {
            tr!(
                "开发构建不检查更新。请使用发布版获取自动更新。",
                "Updates are disabled in development builds. Use a release build to receive updates."
            )
        };
        self.update_status
            .setStringValue(&NSString::from_str(status));
    }
    pub(super) fn update_login_status(&self) {
        // SAFETY: macOS 14+ supports the main application's login service. Status is read-only.
        let status = unsafe { SMAppService::mainAppService().status() };
        let (enabled, text) = match status {
            SMAppServiceStatus::Enabled => (
                true,
                tr!(
                    "已启用；下次登录时启动 Winlane。",
                    "Enabled. Winlane will start when you next log in."
                ),
            ),
            SMAppServiceStatus::RequiresApproval => (
                true,
                tr!(
                    "等待系统允许：请在“管理登录项”中开启。",
                    "Approval needed. Enable Winlane in Manage Login Items."
                ),
            ),
            SMAppServiceStatus::NotFound => (
                false,
                tr!(
                    "尚无可用登录项。启用时会检查应用位置与签名。",
                    "No login item yet. Enabling checks the app's location and signature."
                ),
            ),
            _ => (false, tr!("未启用。", "Disabled.")),
        };
        self.login.setState(if enabled {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        self.login_status.setStringValue(&NSString::from_str(text));
    }
}

pub(super) struct InputPage {
    pub(super) input_method: Retained<NSPopUpButton>,
}

impl InputPage {
    pub(super) fn new(input_tab: &NSView, target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let source = settings_group(
            input_tab,
            tr!("Winlane 输入法", "Winlane input source"),
            570.0,
            112.0,
            mtm,
        );
        source.addSubview(&label(
            tr!("开始输入时使用", "When input begins"),
            14.0,
            rect(20.0, 72.0, 285.0, 24.0),
            mtm,
        ));
        let input_method = popup(
            &[
                tr!("跟随当前输入法", "Keep current input source"),
                tr!("始终英文", "Always English"),
                tr!("始终中文", "Always Chinese"),
                tr!("记住 Winlane 上次使用", "Last used in Winlane"),
            ],
            rect(330.0, 69.0, 390.0, 28.0),
            mtm,
        );
        input_method.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "Winlane 输入法",
            "Winlane input method"
        ))));
        source.addSubview(&input_method);
        source.addSubview(&hint(tr!("统一应用于搜索、命令参数和设置中的输入框；输入过程中仍可手动切换。", "Applies to search, command arguments, and settings fields. You can still change input sources while typing."), rect(20.0, 14.0, 700.0, 44.0), mtm));
        let behavior = settings_group(
            input_tab,
            tr!("输入行为", "How it works"),
            404.0,
            236.0,
            mtm,
        );
        for (title, description, top) in [
            (
                tr!("英文 / 中文", "English / Chinese"),
                tr!(
                    "使用 macOS 对应语言的已启用输入法，支持第三方输入法。",
                    "Use the enabled input source macOS chooses for the language, including third-party input methods."
                ),
                234.0,
            ),
            (
                tr!("记住上次使用", "Last used in Winlane"),
                tr!(
                    "记住上次在 Winlane 中使用的输入法，重启后也会保留。",
                    "Remember the last input source used in Winlane, even after restarting."
                ),
                157.0,
            ),
            (
                tr!("输入法不可用时", "If a source is unavailable"),
                tr!(
                    "保留当前输入法。切换模式中的 alias 不受影响。",
                    "Keep the current input source. Switch-mode aliases are unaffected."
                ),
                80.0,
            ),
        ] {
            row_text(&behavior, title, description, top, 700.0, mtm);
        }
        row_divider(&behavior, 158.0, mtm);
        row_divider(&behavior, 81.0, mtm);

        set_action(&input_method, target, sel!(settingsChanged:));
        Self { input_method }
    }
    pub(super) fn fill(&self, config: &Config) {
        self.input_method
            .selectItemAtIndex(match config.input_method {
                InputMethod::Current => 0,
                InputMethod::English => 1,
                InputMethod::Chinese => 2,
                InputMethod::LastUsed => 3,
            });
    }
}

pub(super) struct WindowsPage {
    pub(super) sort: Retained<NSPopUpButton>,
    pub(super) minimized: Retained<NSButton>,
    pub(super) excluded: Retained<NSTextField>,
    pub(super) switch_delay: Retained<NSTextField>,
}

impl WindowsPage {
    pub(super) fn new(windows: &NSView, target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let listing = settings_group(windows, tr!("窗口列表", "Window list"), 570.0, 130.0, mtm);
        listing.addSubview(&label(
            tr!("窗口排序", "Sort windows"),
            14.0,
            rect(20.0, 91.0, 315.0, 24.0),
            mtm,
        ));
        let sort = popup(
            &[
                tr!("最近在 Winlane 中切换", "Recently switched in Winlane"),
                tr!("应用名称", "Application name"),
                tr!("窗口标题", "Window title"),
            ],
            rect(375.0, 88.0, 345.0, 28.0),
            mtm,
        );
        listing.addSubview(&sort);
        row_divider(&listing, 67.0, mtm);
        let minimized = checkbox(
            tr!("在列表中显示最小化窗口", "Include minimized windows"),
            mtm,
        );
        minimized.setFrame(rect(20.0, 23.0, 690.0, 26.0));
        listing.addSubview(&minimized);
        let exclusions = settings_group(
            windows,
            tr!("排除的应用", "Excluded apps"),
            386.0,
            124.0,
            mtm,
        );
        let excluded =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(20.0, 71.0, 700.0, 28.0));
        excluded.setPlaceholderString(Some(&NSString::from_str(tr!(
            "例如：Finder, Terminal",
            "For example: Finder, Terminal"
        ))));
        exclusions.addSubview(&excluded);
        exclusions.addSubview(&hint(tr!("填写列表中显示的完整应用名，用逗号分隔。这些应用将不出现在候选列表里。", "Enter app names exactly as shown in the list, separated by commas. These apps will be hidden from results."), rect(20.0, 18.0, 700.0, 42.0), mtm));
        let timing = settings_group(windows, tr!("响应速度", "Timing"), 208.0, 102.0, mtm);
        row_text(
            &timing,
            tr!("切换面板显示延迟（毫秒）", "Switcher display delay (ms)"),
            tr!(
                "快速松键直接切换；0 表示立即显示。",
                "Quick releases switch directly; 0 shows the panel immediately."
            ),
            89.0,
            505.0,
            mtm,
        );
        let switch_delay =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(560.0, 38.0, 160.0, 28.0));
        timing.addSubview(&switch_delay);
        for field in [&excluded, &switch_delay] {
            field.cell().unwrap().setSendsActionOnEndEditing(true);
        }
        for control in [
            &*sort as &NSControl,
            &*minimized,
            &*excluded,
            &*switch_delay,
        ] {
            set_action(control, target, sel!(settingsChanged:));
        }
        Self {
            sort,
            minimized,
            excluded,
            switch_delay,
        }
    }
    pub(super) fn fill(&self, config: &Config) {
        self.sort.selectItemAtIndex(match config.sort {
            SortOrder::Recent => 0,
            SortOrder::Application => 1,
            SortOrder::Title => 2,
        });
        self.minimized.setState(if config.include_minimized {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        self.excluded
            .setStringValue(&NSString::from_str(&config.excluded_apps.join(", ")));
        self.switch_delay
            .setStringValue(&NSString::from_str(&config.switch_delay_ms.to_string()));
    }
    pub(super) fn read(&self, config: &mut Config) -> Result<(), String> {
        config.sort = match self.sort.indexOfSelectedItem() {
            1 => SortOrder::Application,
            2 => SortOrder::Title,
            _ => SortOrder::Recent,
        };
        config.include_minimized = self.minimized.state() == NSControlStateValueOn;
        config.excluded_apps = parse_excluded(&self.excluded.stringValue().to_string());
        config.switch_delay_ms = self
            .switch_delay
            .stringValue()
            .to_string()
            .trim()
            .parse()
            .map_err(|_| {
                tr!(
                    "显示延迟应为 0–1000 的整数。",
                    "Display delay must be an integer from 0 to 1000."
                )
            })?;
        Ok(())
    }
}

pub(super) fn build_aliases(aliases: &NSView, target: &AnyObject, mtm: MainThreadMarker) {
    let alias_card = settings_group(
        aliases,
        tr!("应用与项目", "Apps & projects"),
        570.0,
        172.0,
        mtm,
    );
    alias_card.addSubview(&label(
        tr!("固定常用窗口的字母", "Keep familiar aliases"),
        16.0,
        rect(20.0, 130.0, 700.0, 25.0),
        mtm,
    ));
    alias_card.addSubview(&hint(tr!("例如 w → WeChat，ck → Code 的 ckb 项目。\n规则优先于自动分配，重启后保留；标题关键词不区分大小写。", "For example, w → WeChat, ck → the ckb project in Code.\nRules override automatic aliases and survive restarts. Title matching ignores case."), rect(20.0, 68.0, 700.0, 50.0), mtm));
    alias_card.addSubview(&button(
        tr!("配置 Alias 规则…", "Alias Rules…"),
        target,
        sel!(showAliasRules:),
        rect(500.0, 20.0, 220.0, 30.0),
        mtm,
    ));
}
