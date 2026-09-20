use crate::{tr, trf};
use std::time::{Duration, SystemTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Start { minutes: Option<u32>, display: bool },
    Stop,
}

pub const CHOICES: [Choice; 7] = [
    Choice::Start {
        minutes: Some(30),
        display: false,
    },
    Choice::Start {
        minutes: Some(60),
        display: false,
    },
    Choice::Start {
        minutes: None,
        display: false,
    },
    Choice::Start {
        minutes: Some(30),
        display: true,
    },
    Choice::Start {
        minutes: Some(60),
        display: true,
    },
    Choice::Start {
        minutes: None,
        display: true,
    },
    Choice::Stop,
];

impl Choice {
    pub fn title(self) -> String {
        match self {
            Self::Stop => tr!("关闭防休眠", "Turn off Keep Awake").into(),
            Self::Start { minutes: None, .. } => tr!("直到手动关闭", "Until turned off").into(),
            Self::Start {
                minutes: Some(minutes),
                ..
            } => trf!("{} 分钟", "{} minutes", minutes),
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Self::Stop => tr!("恢复系统正常休眠", "Resume normal sleep behavior"),
            Self::Start { display: true, .. } => {
                tr!("保持系统和屏幕唤醒", "Keep Mac and display awake")
            }
            Self::Start { display: false, .. } => tr!(
                "保持系统唤醒，允许屏幕熄灭",
                "Keep Mac awake; allow display sleep"
            ),
        }
    }

    pub fn deadline(self, now: SystemTime) -> Option<SystemTime> {
        match self {
            Self::Start {
                minutes: Some(minutes),
                ..
            } => now.checked_add(Duration::from_secs(u64::from(minutes) * 60)),
            _ => None,
        }
    }
}

pub fn matching(query: &str) -> Vec<Choice> {
    let terms: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
    CHOICES
        .into_iter()
        .filter(|choice| {
            let text = format!("{} {}", choice.title(), choice.detail()).to_lowercase();
            terms.iter().all(|term| text.contains(term))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndicatorStatus {
    pub display: bool,
    pub remaining_minutes: Option<u64>,
}

impl IndicatorStatus {
    pub fn new(choice: Choice, deadline: Option<SystemTime>, now: SystemTime) -> Option<Self> {
        let Choice::Start { display, .. } = choice else {
            return None;
        };
        let remaining_minutes = if let Some(end) = deadline {
            let remaining = end
                .duration_since(now)
                .ok()
                .filter(|duration| !duration.is_zero())?;
            // A partial final second still represents an active session.
            Some((remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0)).div_ceil(60))
        } else {
            None
        };
        Some(Self {
            display,
            remaining_minutes,
        })
    }

    pub fn label(self) -> String {
        let mode = if self.display {
            tr!("屏幕常亮", "Display on")
        } else {
            tr!("防休眠", "Awake")
        };
        let duration = self.remaining_minutes.map_or_else(
            || tr!("持续", "Until off").into(),
            |minutes| trf!("{} 分钟", "{}m", minutes),
        );
        format!("☕ {mode} · {duration}")
    }
}

pub fn indicator_frame(
    safe: crate::core::displays::Rect,
    text_width: f64,
    occupied: &[crate::core::displays::Rect],
) -> crate::core::displays::Rect {
    use crate::core::displays::Rect;
    let width = (text_width + 20.0).clamp(100.0, 280.0).min(safe.width);
    let height = 24.0_f64.min(safe.height);
    let frame = Rect {
        x: safe.x + (safe.width - width - 8.0).max(0.0),
        y: safe.y + (safe.height - height - 8.0).max(0.0),
        width,
        height,
    };
    let overlaps = |candidate: Rect| {
        occupied.iter().any(|other| {
            candidate.x < other.x + other.width
                && candidate.x + candidate.width > other.x
                && candidate.y < other.y + other.height
                && candidate.y + candidate.height > other.y
        })
    };
    if !overlaps(frame) {
        return frame;
    }
    // Try below, then left of input indicators without leaving the usable screen.
    occupied
        .iter()
        .flat_map(|other| {
            [
                Rect {
                    y: other.y - height - 6.0,
                    ..frame
                },
                Rect {
                    x: other.x - width - 6.0,
                    ..frame
                },
            ]
        })
        .find(|candidate| {
            candidate.x >= safe.x
                && candidate.y >= safe.y
                && candidate.x + width <= safe.x + safe.width
                && candidate.y + height <= safe.y + safe.height
                && !overlaps(*candidate)
        })
        .unwrap_or(frame)
}
