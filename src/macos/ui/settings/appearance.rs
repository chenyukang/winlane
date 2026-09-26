use super::*;

pub(super) struct AppearancePage {
    pub(super) appearance: Retained<NSPopUpButton>,
    pub(super) density: Retained<NSPopUpButton>,
    pub(super) panel_display_target: Retained<NSPopUpButton>,
    pub(super) opacity_slider: Retained<NSSlider>,
    pub(super) opacity_input: Retained<NSTextField>,
    pub(super) opacity_preview: crate::macos::ui::material::PanelBackdrop,
    pub(super) usage_hints: Retained<NSButton>,
}

impl AppearancePage {
    pub(super) fn new(appearance_tab: &NSView, target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let display = settings_group(appearance_tab, tr!("显示", "Display"), 570.0, 270.0, mtm);
        display.addSubview(&label(
            tr!("外观", "Appearance"),
            14.0,
            rect(20.0, 228.0, 300.0, 24.0),
            mtm,
        ));
        let appearance = popup(
            &[
                tr!("跟随系统", "System"),
                tr!("浅色", "Light"),
                tr!("深色", "Dark"),
            ],
            rect(420.0, 226.0, 300.0, 28.0),
            mtm,
        );
        display.addSubview(&appearance);
        row_divider(&display, 206.0, mtm);
        row_text(
            &display,
            tr!("显示密度", "Display density"),
            tr!(
                "标准模式使用更大的文字和图标。",
                "Normal uses larger text and icons."
            ),
            202.0,
            380.0,
            mtm,
        );
        let density = popup(
            &[tr!("紧凑", "Compact"), tr!("标准", "Normal")],
            rect(420.0, 151.0, 300.0, 28.0),
            mtm,
        );
        density.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "显示密度",
            "Display density"
        ))));
        display.addSubview(&density);
        row_divider(&display, 129.0, mtm);
        row_text(
            &display,
            tr!("面板显示位置", "Panel displays"),
            tr!(
                "搜索和窗口切换面板显示在哪些屏幕上。",
                "Choose where search and window switching panels appear."
            ),
            125.0,
            380.0,
            mtm,
        );
        let panel_display_target = popup(
            &[
                tr!("所有显示器", "All displays"),
                tr!("活动显示器", "Active display"),
            ],
            rect(420.0, 74.0, 300.0, 28.0),
            mtm,
        );
        panel_display_target.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "面板显示位置",
            "Panel displays"
        ))));
        display.addSubview(&panel_display_target);
        row_divider(&display, 52.0, mtm);
        let usage_hints = checkbox(
            tr!(
                "显示底部提示和设置按钮",
                "Show footer hints and Settings button"
            ),
            mtm,
        );
        usage_hints.setFrame(rect(20.0, 13.0, 690.0, 26.0));
        set_action(&usage_hints, target, sel!(settingsChanged:));
        display.addSubview(&usage_hints);
        let opacity = settings_group(
            appearance_tab,
            tr!("背景不透明度", "Background opacity"),
            246.0,
            196.0,
            mtm,
        );
        let glass = crate::macos::ui::material::glass_available();
        opacity.addSubview(&hint(
            if glass {
                tr!(
                    "正在使用 Liquid Glass，透明度由 macOS 自动调整。",
                    "Liquid Glass is active. macOS adjusts its transparency automatically."
                )
            } else {
                tr!(
                    "降低百分比可透出更多背景；文字和图标保持清晰。",
                    "Lower values reveal more background. Text and icons stay clear."
                )
            },
            rect(20.0, 148.0, 700.0, 30.0),
            mtm,
        ));
        let opacity_slider =
            NSSlider::initWithFrame(NSSlider::alloc(mtm), rect(20.0, 109.0, 594.0, 24.0));
        opacity_slider.setMinValue(0.0);
        opacity_slider.setMaxValue(100.0);
        opacity_slider.setContinuous(true);
        opacity_slider.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "背景不透明度",
            "Background opacity"
        ))));
        let opacity_input =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(634.0, 107.0, 58.0, 26.0));
        opacity_input.setAlignment(NSTextAlignment::Right);
        opacity_input.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "不透明度百分比",
            "Opacity percentage"
        ))));
        set_action(&opacity_slider, target, sel!(changeBackgroundOpacity:));
        set_action(&opacity_input, target, sel!(commitBackgroundOpacity:));
        opacity_slider.setEnabled(!glass);
        opacity_input.setEnabled(!glass);
        opacity.addSubview(&opacity_slider);
        opacity.addSubview(&opacity_input);
        opacity.addSubview(&label("%", 13.0, rect(700.0, 109.0, 20.0, 24.0), mtm));
        let sample = NSView::initWithFrame(NSView::alloc(mtm), rect(20.0, 20.0, 700.0, 64.0));
        for (x, color) in [
            (0.0, NSColor::systemIndigoColor()),
            (350.0, NSColor::systemTealColor()),
        ] {
            let tile = NSBox::initWithFrame(NSBox::alloc(mtm), rect(x, 0.0, 350.0, 64.0));
            tile.setBoxType(NSBoxType::Custom);
            tile.setBorderWidth(0.0);
            tile.setFillColor(&color.colorWithAlphaComponent(0.35));
            sample.addSubview(&tile);
        }
        let opacity_preview = crate::macos::ui::material::PanelBackdrop::new(sample.bounds(), mtm);
        opacity_preview.blend_within_window();
        sample.addSubview(opacity_preview.view());
        opacity_preview.content.addSubview(&label(
            tr!(
                "预览 · 搜索和切换面板",
                "Preview · Search and switch panels"
            ),
            14.0,
            rect(18.0, 20.0, 665.0, 24.0),
            mtm,
        ));
        opacity.addSubview(&sample);

        opacity_input
            .cell()
            .unwrap()
            .setSendsActionOnEndEditing(true);
        for control in [&*appearance, &*density, &*panel_display_target] {
            set_action(control, target, sel!(settingsChanged:));
        }
        Self {
            appearance,
            density,
            panel_display_target,
            opacity_slider,
            opacity_input,
            opacity_preview,
            usage_hints,
        }
    }
    pub(super) fn fill(&self, config: &Config) {
        self.appearance.selectItemAtIndex(match config.appearance {
            Appearance::System => 0,
            Appearance::Light => 1,
            Appearance::Dark => 2,
        });
        self.density
            .selectItemAtIndex(match config.display_density {
                DisplayDensity::Compact => 0,
                DisplayDensity::Normal => 1,
            });
        self.panel_display_target
            .selectItemAtIndex(match config.panel_display_target {
                PanelDisplayTarget::AllDisplays => 0,
                PanelDisplayTarget::ActiveDisplay => 1,
            });
        self.usage_hints.setState(if config.show_usage_hints {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        self.set_opacity(config.background_opacity);
    }
    pub(super) fn read(&self, config: &mut Config) -> Result<(), String> {
        config.appearance = match self.appearance.indexOfSelectedItem() {
            1 => Appearance::Light,
            2 => Appearance::Dark,
            _ => Appearance::System,
        };
        config.display_density = match self.density.indexOfSelectedItem() {
            0 => DisplayDensity::Compact,
            _ => DisplayDensity::Normal,
        };
        config.panel_display_target = match self.panel_display_target.indexOfSelectedItem() {
            1 => PanelDisplayTarget::ActiveDisplay,
            _ => PanelDisplayTarget::AllDisplays,
        };
        config.show_usage_hints = self.usage_hints.state() == NSControlStateValueOn;
        config.background_opacity = self.read_opacity()?;
        Ok(())
    }
    pub(super) fn read_opacity(&self) -> Result<u8, String> {
        self.opacity_input
            .stringValue()
            .to_string()
            .trim()
            .parse::<u8>()
            .ok()
            .filter(|value| *value <= 100)
            .ok_or_else(|| {
                tr!(
                    "请输入 0–100 之间的整数百分比。",
                    "Enter a whole-number percentage from 0 to 100."
                )
                .into()
            })
    }
    pub(super) fn set_opacity(&self, value: u8) {
        self.opacity_slider.setDoubleValue(f64::from(value));
        self.opacity_input
            .setStringValue(&NSString::from_str(&value.to_string()));
        self.opacity_preview.set_opacity(f64::from(value) / 100.0);
    }
}
