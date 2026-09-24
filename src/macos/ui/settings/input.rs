use super::*;
use crate::macos::platform::input_source::Source;
use crate::macos::ui::input_indicator::native_color;
use winlane::features::input_indicator::{
    Color, DisplayTarget, InputSource, Position, Size, Style,
};

pub(super) struct InputPage {
    pub(super) enabled: Retained<NSButton>,
    pub(super) style: Retained<NSPopUpButton>,
    pub(super) position: Retained<NSPopUpButton>,
    pub(super) size: Retained<NSPopUpButton>,
    pub(super) length: Retained<NSPopUpButton>,
    pub(super) displays: Retained<NSPopUpButton>,
    pub(super) source: Retained<NSPopUpButton>,
    source_visible: Retained<NSButton>,
    pub(super) color: Retained<NSColorWell>,
    reset: Retained<NSButton>,
    sources: RefCell<Vec<InputSource>>,
    shape_width: Retained<NSTextField>,
    shape_height: Retained<NSTextField>,
    offset_x: Retained<NSTextField>,
    offset_y: Retained<NSTextField>,
    pub(super) time_enabled: Retained<NSButton>,
    pub(super) time_position: Retained<NSPopUpButton>,
    pub(super) time_size: Retained<NSPopUpButton>,
    pub(super) time_displays: Retained<NSPopUpButton>,
    pub(super) time_color: Retained<NSColorWell>,
    time_reset: Retained<NSButton>,
    time_offset_x: Retained<NSTextField>,
    time_offset_y: Retained<NSTextField>,
}

impl InputPage {
    pub(super) fn new(host: &NSView, target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), host.bounds());
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setScrollerStyle(NSScrollerStyle::Overlay);
        scroll.setDrawsBackground(false);
        let document = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 740.0, 732.0));
        scroll.setDocumentView(Some(&document));
        host.addSubview(&scroll);
        let host = &*document;
        let group = settings_group(host, tr!("时间指示器", "Time indicator"), 724.0, 206.0, mtm);
        let time_enabled = checkbox(tr!("持续显示当前时间", "Always show the current time"), mtm);
        time_enabled.setFrame(rect(20.0, 158.0, 700.0, 27.0));
        group.addSubview(&time_enabled);
        row_divider(&group, 148.0, mtm);
        let time_position = choice(
            &group,
            tr!("位置", "Position"),
            &[
                tr!("顶部居中", "Top center"),
                tr!("底部居中", "Bottom center"),
                tr!("左侧居中", "Left center"),
                tr!("右侧居中", "Right center"),
                tr!("左上角", "Top left"),
                tr!("右上角", "Top right"),
                tr!("左下角", "Bottom left"),
                tr!("右下角", "Bottom right"),
            ],
            20.0,
            106.0,
            mtm,
        );
        let time_size = choice(
            &group,
            tr!("大小", "Size"),
            &[
                tr!("小", "Small"),
                tr!("标准", "Normal"),
                tr!("大", "Large"),
            ],
            380.0,
            106.0,
            mtm,
        );
        let time_displays = choice(
            &group,
            tr!("显示器", "Displays"),
            &[
                tr!("所有显示器", "All displays"),
                tr!("主显示器", "Main display"),
                tr!("非主显示器", "Non-main display"),
            ],
            20.0,
            61.0,
            mtm,
        );
        let time_color =
            NSColorWell::initWithFrame(NSColorWell::alloc(mtm), rect(524.0, 60.0, 66.0, 30.0));
        time_color.setColorWellStyle(NSColorWellStyle::Minimal);
        time_color.setSupportsAlpha(false);
        time_color.setAccessibilityLabel(Some(&NSString::from_str(tr!("时间颜色", "Time color"))));
        group.addSubview(&time_color);
        let time_reset = button(
            tr!("恢复默认", "Reset color"),
            target,
            sel!(resetTimeIndicatorColor:),
            rect(602.0, 59.0, 120.0, 30.0),
            mtm,
        );
        group.addSubview(&time_reset);
        let time_offset_x = number(&group, tr!("X 偏移", "X offset"), 20.0, 16.0, mtm);
        let time_offset_y = number(&group, tr!("Y 偏移", "Y offset"), 380.0, 16.0, mtm);
        let group = settings_group(
            host,
            tr!("输入法指示器", "Input source indicator"),
            462.0,
            414.0,
            mtm,
        );
        let enabled = checkbox(
            tr!("持续显示当前输入法", "Always show the current input source"),
            mtm,
        );
        enabled.setFrame(rect(20.0, 367.0, 700.0, 27.0));
        group.addSubview(&enabled);
        row_divider(&group, 357.0, mtm);
        let style = choice(
            &group,
            tr!("外观", "Style"),
            &[
                tr!("细色条", "Color bar"),
                tr!("名称色块", "Name badge"),
                tr!("圆形", "Circle"),
                tr!("圆角矩形", "Rounded rectangle"),
            ],
            20.0,
            317.0,
            mtm,
        );
        let position = choice(
            &group,
            tr!("位置", "Position"),
            &[
                tr!("顶部居中", "Top center"),
                tr!("底部居中", "Bottom center"),
                tr!("左侧居中", "Left center"),
                tr!("右侧居中", "Right center"),
                tr!("左上角", "Top left"),
                tr!("右上角", "Top right"),
                tr!("左下角", "Bottom left"),
                tr!("右下角", "Bottom right"),
            ],
            380.0,
            317.0,
            mtm,
        );
        let size = choice(
            &group,
            tr!("大小", "Size"),
            &[
                tr!("小", "Small"),
                tr!("标准", "Normal"),
                tr!("大", "Large"),
            ],
            20.0,
            272.0,
            mtm,
        );
        let length = choice(
            &group,
            tr!("色条长度", "Bar length"),
            &["25%", "50%", "100%"],
            380.0,
            272.0,
            mtm,
        );
        let displays = choice(
            &group,
            tr!("显示器", "Displays"),
            &[
                tr!("所有显示器", "All displays"),
                tr!("主显示器", "Main display"),
                tr!("非主显示器", "Non-main display"),
            ],
            20.0,
            227.0,
            mtm,
        );
        let shape_width = number(
            &group,
            tr!("宽 / 直径", "Width / diameter"),
            20.0,
            182.0,
            mtm,
        );
        let shape_height = number(&group, tr!("高", "Height"), 380.0, 182.0, mtm);
        let offset_x = number(&group, tr!("X 偏移", "X offset"), 20.0, 137.0, mtm);
        let offset_y = number(&group, tr!("Y 偏移", "Y offset"), 380.0, 137.0, mtm);
        row_divider(&group, 122.0, mtm);
        let source = popup(&[], rect(20.0, 79.0, 480.0, 28.0), mtm);
        source.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "要配置的输入法",
            "Input source to customize"
        ))));
        group.addSubview(&source);
        let color =
            NSColorWell::initWithFrame(NSColorWell::alloc(mtm), rect(524.0, 78.0, 66.0, 30.0));
        color.setColorWellStyle(NSColorWellStyle::Minimal);
        color.setSupportsAlpha(false);
        color.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "输入法颜色",
            "Input source color"
        ))));
        group.addSubview(&color);
        let reset = button(
            tr!("恢复默认", "Reset color"),
            target,
            sel!(resetIndicatorColor:),
            rect(602.0, 78.0, 120.0, 30.0),
            mtm,
        );
        group.addSubview(&reset);
        let source_visible = checkbox(
            tr!(
                "显示此输入法的指示器",
                "Show the indicator for this input source"
            ),
            mtm,
        );
        source_visible.setFrame(rect(20.0, 44.0, 700.0, 27.0));
        group.addSubview(&source_visible);
        group.addSubview(&hint(
            tr!(
                "关闭后，使用此输入法时不显示；颜色设置会保留。",
                "Turn off to hide the indicator for this source. Its color is kept."
            ),
            rect(20.0, 7.0, 700.0, 30.0),
            mtm,
        ));

        for control in [
            &*enabled as &NSControl,
            &*style,
            &*position,
            &*size,
            &*length,
            &*displays,
            &*color,
            &*source_visible,
            &*shape_width,
            &*shape_height,
            &*offset_x,
            &*offset_y,
            &*time_enabled,
            &*time_position,
            &*time_size,
            &*time_displays,
            &*time_color,
            &*time_offset_x,
            &*time_offset_y,
        ] {
            set_action(control, target, sel!(settingsChanged:));
        }
        set_action(&source, target, sel!(indicatorSourceSelected:));
        set_action(&time_reset, target, sel!(resetTimeIndicatorColor:));
        document.scrollRectToVisible(rect(0.0, document.bounds().size.height - 1.0, 740.0, 1.0));
        Self {
            enabled,
            style,
            position,
            size,
            length,
            displays,
            source,
            source_visible,
            color,
            reset,
            sources: RefCell::new(Vec::new()),
            shape_width,
            shape_height,
            offset_x,
            offset_y,
            time_enabled,
            time_position,
            time_size,
            time_displays,
            time_color,
            time_reset,
            time_offset_x,
            time_offset_y,
        }
    }

    pub(super) fn fill(&self, config: &Config) {
        let indicator = &config.input_indicator;
        self.enabled.setState(if indicator.enabled {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        self.style.selectItemAtIndex(
            Style::ALL
                .iter()
                .position(|style| *style == indicator.style)
                .unwrap_or(0) as isize,
        );
        for (field, value) in [
            (&self.shape_width, i32::from(indicator.shape_width)),
            (&self.shape_height, i32::from(indicator.shape_height)),
            (&self.offset_x, indicator.offset_x),
            (&self.offset_y, indicator.offset_y),
        ] {
            field.setStringValue(&NSString::from_str(&value.to_string()));
        }
        self.position.selectItemAtIndex(
            Position::ALL
                .iter()
                .position(|p| *p == indicator.position)
                .unwrap_or(0) as isize,
        );
        self.size.selectItemAtIndex(match indicator.size {
            Size::Small => 0,
            Size::Medium => 1,
            Size::Large => 2,
        });
        self.length
            .selectItemAtIndex(match indicator.bar_length_percent {
                25 => 0,
                50 => 1,
                _ => 2,
            });
        self.displays
            .selectItemAtIndex(match indicator.display_target {
                DisplayTarget::AllDisplays => 0,
                DisplayTarget::MainDisplay => 1,
                DisplayTarget::NonMainDisplay => 2,
            });
        let time = &config.time_indicator;
        self.time_enabled.setState(if time.enabled {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        self.time_position.selectItemAtIndex(
            Position::ALL
                .iter()
                .position(|position| *position == time.position)
                .unwrap_or(0) as isize,
        );
        self.time_size.selectItemAtIndex(match time.size {
            Size::Small => 0,
            Size::Medium => 1,
            Size::Large => 2,
        });
        self.time_displays
            .selectItemAtIndex(match time.display_target {
                DisplayTarget::AllDisplays => 0,
                DisplayTarget::MainDisplay => 1,
                DisplayTarget::NonMainDisplay => 2,
            });
        self.time_offset_x
            .setStringValue(&NSString::from_str(&time.offset_x.to_string()));
        self.time_offset_y
            .setStringValue(&NSString::from_str(&time.offset_y.to_string()));
        self.time_color.setColor(&native_color(time.color));
        self.refresh_sources(config);
        self.sync_controls();
    }

    pub(super) fn sync_controls(&self) {
        let style = self.selected_style();
        self.length.setEnabled(style == Style::Bar);
        self.size.setEnabled(!style.is_shape());
        self.shape_width.setEnabled(style.is_shape());
        self.shape_height
            .setEnabled(style == Style::RoundedRectangle);
        let available = self.selected_source().is_some();
        self.source_visible.setEnabled(available);
        let visible = available && self.source_visible.state() == NSControlStateValueOn;
        if !visible {
            self.color.deactivate();
        }
        self.color.setEnabled(visible);
        self.reset.setEnabled(visible);
        self.sync_time_controls();
    }

    fn sync_time_controls(&self) {
        let enabled = self.time_enabled.state() == NSControlStateValueOn;
        self.time_position.setEnabled(enabled);
        self.time_size.setEnabled(enabled);
        self.time_displays.setEnabled(enabled);
        self.time_color.setEnabled(enabled);
        self.time_reset.setEnabled(enabled);
        self.time_offset_x.setEnabled(enabled);
        self.time_offset_y.setEnabled(enabled);
        if !enabled {
            self.time_color.deactivate();
        }
    }

    pub(super) fn refresh_sources(&self, config: &Config) {
        let selected = self.selected_source().map(|source| source.id);
        let current = Source::current(self.source.mtm()).and_then(|source| source.description());
        let mut sources = Source::enabled(self.source.mtm());
        if let Some(current) = &current
            && !sources.iter().any(|source| source.id == current.id)
        {
            sources.push(current.clone());
        }
        let selected = selected.or_else(|| current.map(|source| source.id));
        self.source.removeAllItems();
        for source in &sources {
            // Distinct IDs may share a display name; keep each independently configurable.
            let duplicate = sources
                .iter()
                .filter(|other| other.name == source.name)
                .count()
                > 1;
            let title = if duplicate {
                format!("{} ({})", source.name, source.id)
            } else {
                source.name.clone()
            };
            self.source.addItemWithTitle(&NSString::from_str(&title));
        }
        let index = selected
            .and_then(|id| sources.iter().position(|source| source.id == id))
            .unwrap_or(0);
        let available = !sources.is_empty();
        self.sources.replace(sources);
        if available {
            self.source.selectItemAtIndex(index as isize);
        }
        self.source.setEnabled(available);
        self.select_source(config);
    }

    fn selected_source(&self) -> Option<InputSource> {
        usize::try_from(self.source.indexOfSelectedItem())
            .ok()
            .and_then(|index| self.sources.borrow().get(index).cloned())
    }

    fn selected_style(&self) -> Style {
        Style::ALL
            .get(self.style.indexOfSelectedItem() as usize)
            .copied()
            .unwrap_or_default()
    }

    pub(super) fn select_source(&self, config: &Config) {
        self.color.deactivate();
        if let Some(source) = self.selected_source() {
            self.source_visible.setState(
                if config.input_indicator.hidden_sources.contains(&source.id) {
                    NSControlStateValueOff
                } else {
                    NSControlStateValueOn
                },
            );
            self.color
                .setColor(&native_color(config.input_indicator.color(&source)));
        }
        self.sync_controls();
    }

    pub(super) fn reset_color(&self) {
        if let Some(source) = self.selected_source() {
            self.color
                .setColor(&native_color(Color::for_source(&source)));
        }
    }

    pub(super) fn reset_time_color(&self) {
        self.time_color.setColor(&native_color(Color::time()));
    }

    pub(super) fn read(&self, config: &mut Config) -> Result<(), String> {
        let settings = &mut config.input_indicator;
        settings.enabled = self.enabled.state() == NSControlStateValueOn;
        settings.style = self.selected_style();
        // Inactive fields retain their saved value, even if they contain an unfinished edit.
        if settings.style.is_shape() {
            settings.shape_width = read_number(&self.shape_width, 4, 512)? as u16;
            if settings.style == Style::RoundedRectangle {
                settings.shape_height = read_number(&self.shape_height, 4, 512)? as u16;
            }
        }
        settings.offset_x = read_number(&self.offset_x, -10000, 10000)?;
        settings.offset_y = read_number(&self.offset_y, -10000, 10000)?;
        settings.position = Position::ALL
            .get(self.position.indexOfSelectedItem() as usize)
            .copied()
            .unwrap_or_default();
        settings.size = match self.size.indexOfSelectedItem() {
            0 => Size::Small,
            2 => Size::Large,
            _ => Size::Medium,
        };
        settings.bar_length_percent = match self.length.indexOfSelectedItem() {
            0 => 25,
            1 => 50,
            _ => 100,
        };
        settings.display_target = match self.displays.indexOfSelectedItem() {
            0 => DisplayTarget::AllDisplays,
            2 => DisplayTarget::NonMainDisplay,
            _ => DisplayTarget::MainDisplay,
        };
        if let Some(source) = self.selected_source() {
            if self.source_visible.state() == NSControlStateValueOn {
                settings.hidden_sources.remove(&source.id);
            } else {
                settings.hidden_sources.insert(source.id.clone());
            }
            let color = self
                .color
                .color()
                .colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())
                .ok_or_else(|| {
                    tr!(
                        "无法保存该颜色，请重新选择。",
                        "Could not save this color. Choose another color."
                    )
                    .to_owned()
                })?;
            let component = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            let color = Color(
                component(color.redComponent()),
                component(color.greenComponent()),
                component(color.blueComponent()),
            );
            if color == Color::for_source(&source) {
                settings.colors.remove(&source.id);
            } else {
                settings.colors.insert(source.id, color);
            }
        }
        let time = &mut config.time_indicator;
        time.enabled = self.time_enabled.state() == NSControlStateValueOn;
        if time.enabled {
            time.position = Position::ALL
                .get(self.time_position.indexOfSelectedItem() as usize)
                .copied()
                .unwrap_or_default();
            time.size = match self.time_size.indexOfSelectedItem() {
                0 => Size::Small,
                2 => Size::Large,
                _ => Size::Medium,
            };
            time.display_target = match self.time_displays.indexOfSelectedItem() {
                0 => DisplayTarget::AllDisplays,
                2 => DisplayTarget::NonMainDisplay,
                _ => DisplayTarget::MainDisplay,
            };
            time.offset_x = read_number(&self.time_offset_x, -10000, 10000)?;
            time.offset_y = read_number(&self.time_offset_y, -10000, 10000)?;
            let color = self
                .time_color
                .color()
                .colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())
                .ok_or_else(|| {
                    tr!(
                        "无法保存该颜色，请重新选择。",
                        "Could not save this color. Choose another color."
                    )
                    .to_owned()
                })?;
            let component = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            time.color = Color(
                component(color.redComponent()),
                component(color.greenComponent()),
                component(color.blueComponent()),
            );
        }
        Ok(())
    }
}

impl Drop for InputPage {
    fn drop(&mut self) {
        self.color.deactivate();
    }
}

fn choice(
    parent: &NSView,
    title: &str,
    titles: &[&str],
    x: f64,
    y: f64,
    mtm: MainThreadMarker,
) -> Retained<NSPopUpButton> {
    parent.addSubview(&label(title, 13.0, rect(x, y + 2.0, 88.0, 24.0), mtm));
    let control = popup(titles, rect(x + 90.0, y, 250.0, 28.0), mtm);
    control.setAccessibilityLabel(Some(&NSString::from_str(title)));
    parent.addSubview(&control);
    control
}

fn number(
    parent: &NSView,
    title: &str,
    x: f64,
    y: f64,
    mtm: MainThreadMarker,
) -> Retained<NSTextField> {
    parent.addSubview(&label(title, 13.0, rect(x, y + 2.0, 142.0, 24.0), mtm));
    let field = input(rect(x + 150.0, y, 152.0, 26.0), title, mtm);
    field.setAlignment(NSTextAlignment::Right);
    field.cell().unwrap().setSendsActionOnEndEditing(true);
    parent.addSubview(&field);
    parent.addSubview(&label(
        "pt",
        13.0,
        rect(x + 310.0, y + 2.0, 30.0, 24.0),
        mtm,
    ));
    field
}

fn read_number(field: &NSTextField, min: i32, max: i32) -> Result<i32, String> {
    field
        .stringValue()
        .to_string()
        .trim()
        .parse::<i32>()
        .ok()
        .filter(|value| (min..=max).contains(value))
        .ok_or_else(|| {
            trf!(
                "{}：请输入 {} 到 {} 的整数。",
                "{}: enter a whole number from {} to {}.",
                field
                    .accessibilityLabel()
                    .map(|label| label.to_string())
                    .unwrap_or_default(),
                min,
                max
            )
        })
}

#[cfg(test)]
#[path = "../../../../tests/native/settings_input.rs"]
pub(super) mod tests;
