use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_event_kit::{EKAuthorizationStatus, EKEntityType, EKEventStore};
use objc2_foundation::{NSCalendar, NSDate, NSDateFormatter, NSError, NSString};
use std::sync::mpsc;
use std::time::Duration;
use winlane::features::meeting::{Meeting, Meetings, day_label, extract_link, status_label};
use winlane::tr;

// Reads a day's timed events from the macOS Calendar via EventKit. `day_offset`
// is relative to today (0 = today). EventKit is thread-safe, so this runs off the main thread.
pub fn load(day_offset: i32) -> Meetings {
    let status = unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) };
    let store = unsafe { EKEventStore::new() };
    let authorized = match status {
        EKAuthorizationStatus::FullAccess => true,
        EKAuthorizationStatus::NotDetermined => request_access(&store),
        _ => false,
    };
    if !authorized {
        return Meetings {
            items: Vec::new(),
            error: Some(tr!(
                "需要日历访问权限。在系统设置 → 隐私与安全性 → 日历中允许 Winlane 后重试。",
                "Calendar access is required. Allow Winlane in System Settings → Privacy & Security → Calendars, then retry."
            ).into()),
            access_denied: true,
        };
    }

    let now = NSDate::now();
    let now_ts = now.timeIntervalSince1970();
    let calendar = NSCalendar::currentCalendar();
    let day_start = start_of_day(&calendar, &now, day_offset);
    let day_end = start_of_day(&calendar, &now, day_offset + 1);
    // Today shows only what is still ahead; other days show the whole day.
    let window_start = if day_offset == 0 {
        now.clone()
    } else {
        day_start
    };
    let predicate = unsafe {
        store.predicateForEventsWithStartDate_endDate_calendars(&window_start, &day_end, None)
    };
    let events = unsafe { store.eventsMatchingPredicate(&predicate) };

    let formatter = NSDateFormatter::new();
    formatter.setDateFormat(Some(&NSString::from_str("HH:mm")));

    // Meetings on other days carry their weekday and date; today's rows omit it.
    let day = if day_offset == 0 {
        String::new()
    } else {
        let target = now.dateByAddingTimeInterval(f64::from(day_offset) * 24.0 * 60.0 * 60.0);
        let heading = NSDateFormatter::new();
        heading.setLocalizedDateFormatFromTemplate(&NSString::from_str("EEEMMMd"));
        heading.stringFromDate(&target).to_string()
    };

    let mut items = Vec::new();
    for event in events.iter() {
        if unsafe { event.isAllDay() } {
            continue;
        }
        let start = unsafe { event.startDate() };
        let end_ts = unsafe { event.endDate() }.timeIntervalSince1970();
        // Today hides meetings that already ended; other days keep them, marked done.
        if day_offset == 0 && end_ts <= now_ts {
            continue;
        }
        let start_ts = start.timeIntervalSince1970();
        let mut relative = status_label(
            (start_ts - now_ts).round() as i64,
            (end_ts - now_ts).round() as i64,
        );
        // On other days the date already conveys timing, so drop the countdown for
        // still-upcoming meetings while keeping "in progress" and "completed" markers.
        if day_offset != 0 && start_ts > now_ts {
            relative = String::new();
        }
        let title = unsafe { event.title() }.to_string();
        let title = if title.trim().is_empty() {
            tr!("（无标题）", "(No title)").to_string()
        } else {
            title
        };
        let url = unsafe { event.URL() }
            .and_then(|url| url.absoluteString())
            .map(|url| url.to_string());
        let location = unsafe { event.location() }
            .map(|location| location.to_string())
            .unwrap_or_default();
        let notes = unsafe { event.notes() }
            .map(|notes| notes.to_string())
            .unwrap_or_default();
        let link = extract_link(url.as_deref(), &location, &notes);
        let calendar = unsafe { event.calendar() }
            .map(|calendar| unsafe { calendar.title() }.to_string())
            .unwrap_or_default();
        let start_label = formatter.stringFromDate(&start).to_string();
        let id = unsafe { event.eventIdentifier() }
            .map(|id| id.to_string())
            .unwrap_or_else(|| format!("{start_label}\u{1}{title}"));
        items.push(Meeting {
            id,
            title,
            start_label,
            day: day.clone(),
            relative,
            link,
            calendar,
            location,
        });
    }
    Meetings {
        items,
        error: None,
        access_denied: false,
    }
}

fn request_access(store: &EKEventStore) -> bool {
    let (tx, rx) = mpsc::channel();
    let completion = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
        let _ = tx.send(granted.as_bool());
    });
    // EventKit copies the completion block; the block stays alive until recv returns.
    unsafe {
        store.requestFullAccessToEventsWithCompletion(
            (&*completion as *const block2::Block<_>).cast_mut(),
        )
    };
    rx.recv_timeout(Duration::from_secs(60)).unwrap_or(false)
}

fn start_of_day(calendar: &NSCalendar, now: &NSDate, offset: i32) -> Retained<NSDate> {
    let target = now.dateByAddingTimeInterval(f64::from(offset) * 24.0 * 60.0 * 60.0);
    calendar.startOfDayForDate(&target)
}

// A footer heading for the viewed day; other days add the localized weekday and date.
pub fn day_heading(offset: i32) -> String {
    let name = day_label(offset);
    if offset == 0 {
        return name;
    }
    let target = NSDate::now().dateByAddingTimeInterval(f64::from(offset) * 24.0 * 60.0 * 60.0);
    let formatter = NSDateFormatter::new();
    formatter.setLocalizedDateFormatFromTemplate(&NSString::from_str("EEEMMMd"));
    format!("{name} · {}", formatter.stringFromDate(&target))
}
