use crate::core::displays::Rect;
use crate::tr;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputSource {
    pub id: String,
    pub name: String,
    pub language: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Style {
    #[default]
    Bar,
    Badge,
    Circle,
    RoundedRectangle,
}

impl Style {
    pub const ALL: [Self; 4] = [Self::Bar, Self::Badge, Self::Circle, Self::RoundedRectangle];

    pub fn is_shape(self) -> bool {
        matches!(self, Self::Circle | Self::RoundedRectangle)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Position {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Position {
    pub const ALL: [Self; 8] = [
        Self::Top,
        Self::Bottom,
        Self::Left,
        Self::Right,
        Self::TopLeft,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomRight,
    ];
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Size {
    Small,
    #[default]
    Medium,
    Large,
}

impl Size {
    pub fn thickness(self) -> f64 {
        match self {
            Self::Small => 2.0,
            Self::Medium => 4.0,
            Self::Large => 8.0,
        }
    }
    pub fn font_size(self) -> f64 {
        match self {
            Self::Small => 11.0,
            Self::Medium => 13.0,
            Self::Large => 16.0,
        }
    }
    pub fn badge_height(self) -> f64 {
        match self {
            Self::Small => 24.0,
            Self::Medium => 28.0,
            Self::Large => 34.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayTarget {
    #[default]
    AllDisplays,
    MainDisplay,
    NonMainDisplay,
}

impl DisplayTarget {
    pub const ALL: [Self; 3] = [Self::AllDisplays, Self::MainDisplay, Self::NonMainDisplay];
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DisplayTargetRepr {
    Bool(bool),
    Target(DisplayTarget),
}

fn deserialize_display_target<'de, D>(deserializer: D) -> Result<DisplayTarget, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(match DisplayTargetRepr::deserialize(deserializer)? {
        DisplayTargetRepr::Bool(true) => DisplayTarget::AllDisplays,
        DisplayTargetRepr::Bool(false) => DisplayTarget::MainDisplay,
        DisplayTargetRepr::Target(target) => target,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    pub fn for_source(source: &InputSource) -> Self {
        match source
            .language
            .as_deref()
            .unwrap_or("")
            .split(['-', '_'])
            .next()
            .unwrap_or("")
        {
            "en" => Self(35, 130, 245),
            "zh" => Self(235, 90, 55),
            "ja" => Self(150, 85, 210),
            "ko" => Self(20, 150, 95),
            _ => Self(15, 145, 160),
        }
    }

    pub fn time() -> Self {
        Self(45, 130, 235)
    }

    pub fn dark_text(self) -> bool {
        let linear = |component: u8| {
            let s = f64::from(component) / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(self.0) + 0.7152 * linear(self.1) + 0.0722 * linear(self.2) > 0.179
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub style: Style,
    pub position: Position,
    pub size: Size,
    pub bar_length_percent: u8,
    #[serde(
        alias = "all_displays",
        default,
        deserialize_with = "deserialize_display_target"
    )]
    pub display_target: DisplayTarget,
    pub colors: BTreeMap<String, Color>,
    pub hidden_sources: BTreeSet<String>,
    pub shape_width: u16,
    pub shape_height: u16,
    pub offset_x: i32,
    pub offset_y: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            style: Style::Bar,
            position: Position::Top,
            size: Size::Medium,
            bar_length_percent: 100,
            display_target: DisplayTarget::AllDisplays,
            colors: BTreeMap::new(),
            hidden_sources: BTreeSet::new(),
            shape_width: 20,
            shape_height: 12,
            offset_x: 0,
            offset_y: 0,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !(4..=512).contains(&self.shape_width) || !(4..=512).contains(&self.shape_height) {
            return Err(tr!(
                "形状尺寸请输入 4–512 pt 的整数。",
                "Enter whole-number shape dimensions from 4 to 512 pt."
            )
            .into());
        }
        if !(-10000..=10000).contains(&self.offset_x) || !(-10000..=10000).contains(&self.offset_y)
        {
            return Err(tr!(
                "X、Y 偏移请输入 -10000–10000 pt 的整数。",
                "Enter whole-number X and Y offsets from -10000 to 10000 pt."
            )
            .into());
        }
        if ![25, 50, 100].contains(&self.bar_length_percent) {
            return Err(tr!(
                "色条长度请选择 25%、50% 或 100%。",
                "Choose a bar length of 25%, 50%, or 100%."
            )
            .into());
        }
        if self.colors.len() > 256
            || self.hidden_sources.len() > 256
            || self
                .colors
                .keys()
                .chain(self.hidden_sources.iter())
                .any(|id| id.is_empty() || id.len() > 1024)
        {
            return Err(tr!(
                "输入法指示器配置无效。",
                "Invalid input source indicator settings."
            )
            .into());
        }
        Ok(())
    }

    pub fn color(&self, source: &InputSource) -> Color {
        self.colors
            .get(&source.id)
            .copied()
            .unwrap_or_else(|| Color::for_source(source))
    }

    pub fn frame(&self, screen: Rect, safe_area: Rect, text_width: f64) -> Rect {
        let floating = self.style != Style::Bar;
        let area = if floating { safe_area } else { screen };
        let margin = if floating {
            10.0_f64.min(area.width / 4.0).min(area.height / 4.0)
        } else {
            0.0
        };
        let width = (area.width - 2.0 * margin).max(1.0);
        let height = (area.height - 2.0 * margin).max(1.0);
        let fraction = f64::from(self.bar_length_percent) / 100.0;
        let vertical = matches!(self.position, Position::Left | Position::Right);
        let (w, h) = match self.style {
            Style::Badge => (
                (text_width + 24.0).clamp(64.0, 280.0).min(width),
                self.size.badge_height().min(height),
            ),
            Style::Circle => {
                let diameter = f64::from(self.shape_width).min(width).min(height);
                (diameter, diameter)
            }
            Style::RoundedRectangle => (
                f64::from(self.shape_width).min(width),
                f64::from(self.shape_height).min(height),
            ),
            Style::Bar if vertical => (
                self.size.thickness().min(width),
                (height * fraction).min(height),
            ),
            Style::Bar => (
                (width * fraction).min(width),
                self.size.thickness().min(height),
            ),
        };
        let x = match self.position {
            Position::Left | Position::TopLeft | Position::BottomLeft => 0.0,
            Position::Right | Position::TopRight | Position::BottomRight => width - w,
            _ => (width - w) / 2.0,
        };
        let y = match self.position {
            Position::Top | Position::TopLeft | Position::TopRight => height - h,
            Position::Bottom | Position::BottomLeft | Position::BottomRight => 0.0,
            _ => (height - h) / 2.0,
        };
        Rect {
            x: (area.x + margin + x + f64::from(self.offset_x))
                .clamp(screen.x, screen.x + (screen.width - w).max(0.0)),
            // AppKit's Y axis points up; the settings use positive Y to move down.
            y: (area.y + margin + y - f64::from(self.offset_y))
                .clamp(screen.y, screen.y + (screen.height - h).max(0.0)),
            width: w,
            height: h,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimeSettings {
    pub enabled: bool,
    pub position: Position,
    pub size: Size,
    #[serde(
        alias = "all_displays",
        default,
        deserialize_with = "deserialize_display_target"
    )]
    pub display_target: DisplayTarget,
    pub color: Color,
    pub offset_x: i32,
    pub offset_y: i32,
}

impl Default for TimeSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            position: Position::TopRight,
            size: Size::Medium,
            display_target: DisplayTarget::AllDisplays,
            color: Color::time(),
            offset_x: 0,
            offset_y: 0,
        }
    }
}

impl TimeSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !(-10000..=10000).contains(&self.offset_x) || !(-10000..=10000).contains(&self.offset_y)
        {
            return Err(tr!(
                "X、Y 偏移请输入 -10000–10000 pt 的整数。",
                "Enter whole-number X and Y offsets from -10000 to 10000 pt."
            )
            .into());
        }
        Ok(())
    }

    pub fn frame(&self, screen: Rect, safe_area: Rect, text_width: f64) -> Rect {
        let area = safe_area;
        let margin = 10.0_f64.min(area.width / 4.0).min(area.height / 4.0);
        let outer_width = (area.width - 2.0 * margin).max(1.0);
        let outer_height = (area.height - 2.0 * margin).max(1.0);
        let width = (text_width + 24.0).clamp(64.0, 280.0).min(outer_width);
        let height = self.size.badge_height().min(outer_height);
        let x = match self.position {
            Position::Left | Position::TopLeft | Position::BottomLeft => 0.0,
            Position::Right | Position::TopRight | Position::BottomRight => outer_width - width,
            _ => (outer_width - width) / 2.0,
        };
        let y = match self.position {
            Position::Top | Position::TopLeft | Position::TopRight => outer_height - height,
            Position::Bottom | Position::BottomLeft | Position::BottomRight => 0.0,
            _ => (outer_height - height) / 2.0,
        };
        Rect {
            x: (area.x + margin + x + f64::from(self.offset_x))
                .clamp(screen.x, screen.x + (screen.width - width).max(0.0)),
            y: (area.y + margin + y - f64::from(self.offset_y))
                .clamp(screen.y, screen.y + (screen.height - height).max(0.0)),
            width,
            height,
        }
    }
}
