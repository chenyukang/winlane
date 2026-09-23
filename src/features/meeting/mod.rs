use crate::{tr, trf};
use std::sync::OnceLock;

pub const MAX_RESULTS: usize = 100;

/// A single calendar event shown in the meeting list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meeting {
    pub id: String,
    pub title: String,
    pub start_label: String,
    // Localized weekday and date shown for meetings not on the viewed "today"; empty for today.
    pub day: String,
    pub relative: String,
    pub link: Option<String>,
    pub calendar: String,
    pub location: String,
}

/// The result of loading today's meetings, plus any read error.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Meetings {
    pub items: Vec<Meeting>,
    pub error: Option<String>,
    pub access_denied: bool,
}

/// Filter meetings by a query over title, calendar, location, and link.
pub fn matching(items: &[Meeting], query: &str) -> Vec<Meeting> {
    let needle = query.trim().to_lowercase();
    items
        .iter()
        .filter(|meeting| {
            needle.is_empty()
                || meeting.title.to_lowercase().contains(&needle)
                || meeting.calendar.to_lowercase().contains(&needle)
                || meeting.location.to_lowercase().contains(&needle)
                || meeting
                    .link
                    .as_deref()
                    .is_some_and(|link| link.to_lowercase().contains(&needle))
        })
        .take(MAX_RESULTS)
        .cloned()
        .collect()
}

/// A human label for how soon an upcoming meeting starts, e.g. "Starting in 20 minutes".
pub fn relative_label(seconds_until_start: i64) -> String {
    let minutes = (seconds_until_start.max(0) + 30) / 60;
    if minutes < 1 {
        return tr!("即将开始", "Starting now").to_string();
    }
    if minutes < 60 {
        return if minutes == 1 {
            tr!("1 分钟后开始", "Starting in 1 minute").to_string()
        } else {
            trf!("{} 分钟后开始", "Starting in {} minutes", minutes)
        };
    }
    let hours = (minutes + 30) / 60;
    if hours < 24 {
        return if hours == 1 {
            tr!("1 小时后开始", "Starting in 1 hour").to_string()
        } else {
            trf!("{} 小时后开始", "Starting in {} hours", hours)
        };
    }
    let days = (hours + 12) / 24;
    if days == 1 {
        tr!("1 天后开始", "Starting in 1 day").to_string()
    } else {
        trf!("{} 天后开始", "Starting in {} days", days)
    }
}

/// The status shown before each meeting: completed, in progress, or a countdown.
pub fn status_label(seconds_until_start: i64, seconds_until_end: i64) -> String {
    if seconds_until_end <= 0 {
        tr!("已完成", "Completed").to_string()
    } else if seconds_until_start <= 0 {
        tr!("进行中", "In progress").to_string()
    } else {
        relative_label(seconds_until_start)
    }
}

/// A label for the day being viewed, relative to today.
pub fn day_label(offset: i32) -> String {
    match offset {
        0 => tr!("今天", "Today").to_string(),
        1 => tr!("明天", "Tomorrow").to_string(),
        2 => tr!("后天", "In 2 days").to_string(),
        -1 => tr!("昨天", "Yesterday").to_string(),
        -2 => tr!("前天", "2 days ago").to_string(),
        n if n > 0 => trf!("{} 天后", "In {} days", n),
        n => trf!("{} 天前", "{} days ago", -n),
    }
}

/// A joinable meeting link, taken from the event URL first, then its location or notes.
pub fn extract_link(url: Option<&str>, location: &str, notes: &str) -> Option<String> {
    if let Some(url) = url {
        let url = url.trim();
        if is_web_url(url) {
            return Some(url.to_string());
        }
    }
    find_meeting_url(location).or_else(|| find_meeting_url(notes))
}

pub fn is_web_url(text: &str) -> bool {
    (text.starts_with("http://") || text.starts_with("https://"))
        && text.len() > 8
        && !text.chars().any(char::is_whitespace)
}

fn find_meeting_url(text: &str) -> Option<String> {
    static PATTERN: OnceLock<regex::Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| regex::Regex::new(r#"https?://[^\s<>"'\)\]}]+"#).unwrap());
    pattern
        .find_iter(text)
        .map(|found| {
            found
                .as_str()
                .trim_end_matches(['.', ',', ';', ':', ')', ']', '}', '"', '\''])
        })
        .find(|url| is_meeting_provider(url))
        .map(str::to_string)
}

fn is_meeting_provider(url: &str) -> bool {
    const PROVIDERS: &[&str] = &[
        "zoom.us",
        "zoomgov.com",
        "meet.google.com",
        "teams.microsoft.com",
        "teams.live.com",
        "webex.com",
        "whereby.com",
        "meet.jit.si",
        "bluejeans.com",
        "chime.aws",
        "gotomeeting.com",
        "ringcentral.com",
        "meeting.tencent.com",
        "voovmeeting.com",
        "feishu.cn",
        "larksuite.com",
    ];
    let url = url.to_lowercase();
    PROVIDERS.iter().any(|provider| url.contains(provider))
}
