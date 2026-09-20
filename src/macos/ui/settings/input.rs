use super::*;
use crate::macos::platform::input_source::Source;
use crate::macos::ui::input_indicator::native_color;
use winlane::features::input_indicator::{Color, InputSource, Position, Size, Style};

pub(super) struct InputPage {
    pub(super) input_method: Retained<NSPopUpButton>,
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
}

impl InputPage {
    pub(super) fn new(host: &NSView, target: &AnyObject, mtm: MainThreadMarker) -> Self {
        let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), host.bounds());
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setScrollerStyle(NSScrollerStyle::Overlay);
        scroll.setDrawsBackground(false);
        let document = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 740.0, 718.0));
        scroll.setDocumentView(Some(&document));
        host.addSubview(&scroll);
        let host = &*document;
        let group = settings_group(
            host,
            tr!("Winlane 输入法", "Winlane input source"),
            714.0,
            96.0,
            mtm,
        );
        group.addSubview(&label(
            tr!("开始输入时使用", "When input begins"),
            14.0,
            rect(20.0, 57.0, 285.0, 24.0),
            mtm,
        ));
        let input_method = popup(
            &[
                tr!("跟随当前输入法", "Keep current input source"),
                tr!("始终英文", "Always English"),
                tr!("始终中文", "Always Chinese"),
                tr!("记住 Winlane 上次使用", "Last used in Winlane"),
            ],
            rect(330.0, 54.0, 390.0, 28.0),
            mtm,
        );
        input_method.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "Winlane 输入法",
            "Winlane input method"
        ))));
        group.addSubview(&input_method);
        group.addSubview(&hint(
            tr!(
                "应用于所有 Winlane 输入框；输入时仍可手动切换。",
                "Applies to all Winlane fields. You can still switch sources while typing."
            ),
            rect(20.0, 9.0, 700.0, 34.0),
            mtm,
        ));

        let group = settings_group(
            host,
            tr!("屏幕输入法指示器", "On-screen input source indicator"),
            574.0,
            370.0,
            mtm,
        );
        let enabled = checkbox(
            tr!("持续显示当前输入法", "Always show the current input source"),
            mtm,
        );
        enabled.setFrame(rect(20.0, 332.0, 700.0, 27.0));
        group.addSubview(&enabled);
        row_divider(&group, 322.0, mtm);
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
            283.0,
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
            283.0,
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
            238.0,
            mtm,
        );
        let length = choice(
            &group,
            tr!("色条长度", "Bar length"),
            &["25%", "50%", "100%"],
            380.0,
            238.0,
            mtm,
        );
        let displays = choice(
            &group,
            tr!("显示器", "Displays"),
            &[
                tr!("所有显示器", "All displays"),
                tr!("主显示器", "Main display"),
            ],
            20.0,
            193.0,
            mtm,
        );
        let shape_width = number(
            &group,
            tr!("宽 / 直径", "Width / diameter"),
            20.0,
            148.0,
            mtm,
        );
        let shape_height = number(&group, tr!("高", "Height"), 380.0, 148.0, mtm);
        let offset_x = number(&group, tr!("X 偏移", "X offset"), 20.0, 103.0, mtm);
        let offset_y = number(&group, tr!("Y 偏移", "Y offset"), 380.0, 103.0, mtm);
        group.addSubview(&hint(
            tr!("圆形填写直径，圆角矩形填写宽高（4–512 pt）。X 正数向右，Y 正数向下，相对上方所选位置。",
                "Circle uses the diameter; rounded rectangle uses width and height (4–512 pt). Positive X moves right, positive Y moves down from the chosen position."),
            rect(20.0, 43.0, 700.0, 48.0), mtm,
        ));
        group.addSubview(&hint(
            tr!(
                "跟随系统当前输入法，不拦截点击或切换输入法。",
                "Follows the system input source. Clicks pass through."
            ),
            rect(20.0, 7.0, 700.0, 30.0),
            mtm,
        ));

        let group = settings_group(
            host,
            tr!("各输入法设置", "Input source settings"),
            156.0,
            124.0,
            mtm,
        );
        let source = popup(&[], rect(20.0, 77.0, 480.0, 28.0), mtm);
        source.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "要配置的输入法",
            "Input source to customize"
        ))));
        group.addSubview(&source);
        let color =
            NSColorWell::initWithFrame(NSColorWell::alloc(mtm), rect(524.0, 76.0, 66.0, 30.0));
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
            rect(602.0, 76.0, 120.0, 30.0),
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
        source_visible.setFrame(rect(20.0, 42.0, 700.0, 27.0));
        group.addSubview(&source_visible);
        group.addSubview(&hint(
            tr!(
                "关闭后，使用此输入法时不显示；颜色设置会保留。",
                "Turn off to hide the indicator for this source. Its color is kept."
            ),
            rect(20.0, 6.0, 700.0, 30.0),
            mtm,
        ));
        for control in [
            &*input_method as &NSControl,
            &*enabled,
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
        ] {
            set_action(control, target, sel!(settingsChanged:));
        }
        set_action(&source, target, sel!(indicatorSourceSelected:));
        document.scrollRectToVisible(rect(0.0, 717.0, 740.0, 1.0));
        Self {
            input_method,
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
        }
    }

    pub(super) fn fill(&self, config: &Config) {
        self.input_method
            .selectItemAtIndex(match config.input_method {
                InputMethod::Current => 0,
                InputMethod::English => 1,
                InputMethod::Chinese => 2,
                InputMethod::LastUsed => 3,
            });
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
            .selectItemAtIndex(if indicator.all_displays { 0 } else { 1 });
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

    pub(super) fn read(&self, config: &mut Config) -> Result<(), String> {
        config.input_method = match self.input_method.indexOfSelectedItem() {
            0 => InputMethod::Current,
            2 => InputMethod::Chinese,
            3 => InputMethod::LastUsed,
            _ => InputMethod::English,
        };
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
        settings.all_displays = self.displays.indexOfSelectedItem() == 0;
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
