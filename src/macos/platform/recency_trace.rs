pub fn record(event: &str, data: impl FnOnce() -> String) {
    super::logging::record(super::logging::Level::Debug, "recency", event, data);
}

/// Record only slow paths at the normal log level; no debug restart is needed
/// to distinguish slow AX discovery, synchronous preparation and activation.
pub fn record_slow(event: &str, started: std::time::Instant, data: impl FnOnce() -> String) {
    let elapsed = started.elapsed();
    if elapsed >= std::time::Duration::from_millis(50) {
        super::logging::record(super::logging::Level::Info, "switch-timing", event, || {
            format!(
                "elapsed_ms={:.3} {}",
                elapsed.as_secs_f64() * 1000.0,
                data()
            )
        });
    }
}

/// Format a list for the trace with only its first few entries, so a long window
/// list does not turn every event into a kilobyte of log.
pub fn brief<T: std::fmt::Debug>(items: &[T], limit: usize) -> String {
    if items.len() <= limit {
        return format!("{items:?}");
    }
    let head: Vec<&T> = items.iter().take(limit).collect();
    format!("{head:?}+{}", items.len() - limit)
}
