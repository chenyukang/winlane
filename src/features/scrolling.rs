use crate::tr;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub mouse_vertical: bool,
    pub mouse_horizontal: bool,
    pub trackpad_vertical: bool,
    pub trackpad_horizontal: bool,
    pub wheel_step: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            mouse_vertical: true,
            mouse_horizontal: false,
            trackpad_vertical: false,
            trackpad_horizontal: false,
            wheel_step: 0,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.wheel_step > 100 {
            return Err(tr!(
                "滚轮步长应为 0–100 的整数。",
                "Wheel step must be an integer from 0 to 100."
            )
            .into());
        }
        Ok(())
    }

    pub fn multipliers(&self, device: Device, continuous: bool, vertical_lines: i64) -> [i64; 2] {
        if !self.enabled || device == Device::Unknown {
            return [1, 1];
        }
        let (vertical, horizontal) = match device {
            Device::Mouse => (self.mouse_vertical, self.mouse_horizontal),
            Device::Trackpad => (self.trackpad_vertical, self.trackpad_horizontal),
            Device::Unknown => unreachable!(),
        };
        // Keep accelerated wheel events and all touch scrolling at their original scale.
        let step = if device == Device::Mouse && !continuous && vertical_lines.unsigned_abs() == 1 {
            i64::from(self.wheel_step.max(1))
        } else {
            1
        };
        [
            if vertical { -step } else { step },
            if horizontal { -1 } else { 1 },
        ]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Device {
    Mouse,
    Trackpad,
    #[default]
    Unknown,
}

#[derive(Default)]
pub struct Classifier {
    touch_at: Option<u64>,
    continuous_at: Option<u64>,
    continuous_device: Device,
}

impl Classifier {
    pub fn touch(&mut self, fingers: usize, now_ms: u64) {
        if fingers >= 2 {
            self.touch_at = Some(now_ms);
        }
    }

    pub fn classify(&mut self, continuous: bool, momentum: bool, now_ms: u64) -> Device {
        if !continuous {
            // A wheel event during trackpad inertia must not change its source.
            return Device::Mouse;
        }
        let device = if momentum {
            if self
                .continuous_at
                .is_some_and(|last| now_ms.saturating_sub(last) <= 2000)
            {
                self.continuous_device
            } else {
                Device::Unknown
            }
        } else if self
            .touch_at
            .is_some_and(|last| now_ms.saturating_sub(last) <= 250)
        {
            Device::Trackpad
        } else {
            Device::Mouse
        };
        self.continuous_at = Some(now_ms);
        self.continuous_device = device;
        device
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Delta {
    pub lines: i64,
    pub points: i64,
    pub fixed: f64,
}

impl Delta {
    pub fn scaled(self, multiplier: i64) -> Self {
        Self {
            lines: self.lines.saturating_mul(multiplier),
            points: self.points.saturating_mul(multiplier),
            fixed: self.fixed * multiplier as f64,
        }
    }
}
