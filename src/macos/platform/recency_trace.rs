pub fn record(event: &str, data: impl FnOnce() -> String) {
    super::logging::record(super::logging::Level::Debug, "recency", event, data);
}
