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
