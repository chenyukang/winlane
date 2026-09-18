//! Opt-in, bounded input-start diagnostics. Never records key codes or text.

use std::cell::RefCell;
use std::fmt::Arguments;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[derive(Default)]
struct Trace {
    file: Option<File>,
    start: Option<Instant>,
    sessions: u32,
    lines: u32,
}

thread_local! {
    static TRACE: RefCell<Trace> = RefCell::new(Trace::default());
}

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("WINLANE_TRACE_INPUT").is_ok_and(|v| v == "1"))
}

pub fn start() {
    if !enabled() {
        return;
    }
    TRACE.with_borrow_mut(|trace| {
        if trace.sessions >= 10 {
            trace.start = None;
            return;
        }
        if trace.file.is_none() && trace.sessions == 0 {
            trace.file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(format!(
                    "/tmp/winlane-input-trace-{}.log",
                    std::process::id()
                ))
                .ok();
        }
        trace.sessions += 1;
        trace.lines = 0;
        trace.start = Some(Instant::now());
    });
    record("start", format_args!(""));
}

pub fn active() -> bool {
    enabled()
        && TRACE.with_borrow(|trace| {
            trace.file.is_some()
                && trace.lines < 300
                && trace
                    .start
                    .is_some_and(|start| start.elapsed() < Duration::from_secs(2))
        })
}

pub fn record(event: &'static str, detail: Arguments<'_>) {
    if !active() {
        return;
    }
    TRACE.with_borrow_mut(|trace| {
        let elapsed = trace.start.unwrap().elapsed().as_secs_f64() * 1000.0;
        let session = trace.sessions;
        trace.lines += 1;
        if let Some(file) = &mut trace.file {
            let _ = writeln!(file, "session={session} ms={elapsed:.3} {event} {detail}");
        }
    });
}
