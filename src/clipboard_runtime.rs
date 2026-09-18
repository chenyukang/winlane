use objc2::rc::Retained;
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString, NSWorkspace};
use objc2_foundation::{
    NSData, NSDate, NSDateFormatter, NSDateFormatterStyle, NSHomeDirectory, NSString, ns_string,
};
use std::path::PathBuf;
use winlane::clipboard::{ClipboardSettings, Entry, History, ignored};
use winlane::clipboard_store::Store;
use winlane::tr;

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn history_path() -> PathBuf {
    PathBuf::from(NSHomeDirectory().to_string())
        .join("Library/Application Support/Winlane/Clipboard/history.json")
}

pub struct Observer {
    count: isize,
}
impl Observer {
    pub fn new(board: &NSPasteboard) -> Self {
        Self {
            count: board.changeCount(),
        }
    }
    pub fn changed(&self, board: &NSPasteboard) -> bool {
        self.count != board.changeCount()
    }
    pub fn reset(&mut self, board: &NSPasteboard) {
        self.count = board.changeCount();
    }
    pub fn capture(
        &mut self,
        board: &NSPasteboard,
        history: &mut History,
        settings: &ClipboardSettings,
        source_id: &str,
        source_name: &str,
        time: u64,
    ) -> bool {
        let count = board.changeCount();
        if count == self.count {
            return false;
        }
        self.count = count;
        if !settings.enabled {
            return false;
        }
        let types: Vec<_> = board
            .types()
            .map(|types| types.iter().map(|kind| kind.to_string()).collect())
            .unwrap_or_default();
        if ignored(&types, source_id) || !types.iter().any(|kind| kind == "public.utf8-plain-text")
        {
            return false;
        }
        let Some(text) = board.stringForType(unsafe { NSPasteboardTypeString }) else {
            return false;
        };
        if text.length() > winlane::clipboard::MAX_ITEM_BYTES {
            return false;
        }
        if board.changeCount() != count {
            return false;
        }
        history.record(&text.to_string(), source_name, time, settings)
    }
}

pub struct ClipboardRuntime {
    pub history: History,
    pub error: Option<String>,
    settings: ClipboardSettings,
    observer: Observer,
    board: Retained<NSPasteboard>,
    store: Store,
    pub loading: bool,
    ignore_loaded: bool,
}
impl ClipboardRuntime {
    pub fn new(settings: ClipboardSettings) -> Self {
        Self::with_board(settings, NSPasteboard::generalPasteboard(), history_path())
    }
    pub fn with_board(
        settings: ClipboardSettings,
        board: Retained<NSPasteboard>,
        path: PathBuf,
    ) -> Self {
        Self {
            history: History::default(),
            error: None,
            observer: Observer::new(&board),
            board,
            store: Store::new(path, now(), settings.clone()),
            settings,
            loading: true,
            ignore_loaded: false,
        }
    }
    pub fn poll_storage(&mut self) -> bool {
        let mut changed = false;
        if self.loading
            && let Ok(result) = self.store.loaded.try_recv()
        {
            self.loading = false;
            if !self.ignore_loaded {
                match result {
                    Ok(mut history) => {
                        history.prune(now(), &self.settings);
                        self.history = history;
                        self.persist();
                    }
                    Err(_) => self.error = Some(tr!("历史文件无法读取，原文件已保留；可清空历史后重新记录。", "History could not be loaded. Its file was preserved; clear history to start again.").into()),
                }
            }
            changed = true;
        }
        while self.store.errors.try_recv().is_ok() {
            self.error = Some(
                tr!(
                    "历史无法保存到本机，当前记录仅保留在内存。",
                    "History could not be saved. Current entries remain in memory only."
                )
                .into(),
            );
            changed = true;
        }
        if self.history.prune(now(), &self.settings) {
            self.persist();
            changed = true;
        }
        changed
    }
    pub fn poll_clipboard(&mut self) -> bool {
        let mut changed = self.poll_storage();
        if self.loading || !self.settings.enabled || !self.observer.changed(&self.board) {
            return changed;
        }
        let app = NSWorkspace::sharedWorkspace().frontmostApplication();
        let id = app
            .as_ref()
            .and_then(|app| app.bundleIdentifier())
            .map(|id| id.to_string())
            .unwrap_or_default();
        let name = app
            .and_then(|app| app.localizedName())
            .map(|name| name.to_string())
            .unwrap_or_default();
        let captured = self.observer.capture(
            &self.board,
            &mut self.history,
            &self.settings,
            &id,
            &name,
            now(),
        );
        let pruned = self.history.prune(now(), &self.settings);
        if captured || pruned {
            self.persist();
            changed = true;
        }
        changed
    }
    pub fn configure(&mut self, settings: ClipboardSettings) {
        if self.settings == settings {
            return;
        }
        self.observer.reset(&self.board);
        self.settings = settings;
        self.history.prune(now(), &self.settings);
        if !self.loading || !self.settings.persistent {
            self.persist();
        }
    }
    fn persist(&self) {
        self.store
            .save(self.settings.persistent.then(|| self.history.clone()));
    }
    pub fn remove(&mut self, id: u64) {
        if self.history.remove(id) {
            self.persist();
        }
    }
    pub fn clear(&mut self) {
        self.ignore_loaded = true;
        self.history.entries.clear();
        self.error = None;
        self.observer.reset(&self.board);
        self.store.save(None);
    }
}

pub fn summary(entry: &Entry) -> (String, String, String) {
    let date = NSDate::dateWithTimeIntervalSince1970(entry.copied_at as f64);
    thread_local! {
        static FORMATTER: Retained<NSDateFormatter> = {
            let formatter = NSDateFormatter::new();
            formatter.setDateStyle(NSDateFormatterStyle::ShortStyle);
            formatter.setTimeStyle(NSDateFormatterStyle::ShortStyle);
            formatter
        };
    }
    let time = FORMATTER.with(|formatter| formatter.stringFromDate(&date).to_string());
    let source = if entry.source.is_empty() {
        tr!("文本", "Text")
    } else {
        &entry.source
    };
    (
        source.into(),
        entry.preview(),
        format!(
            "{source} · {time}\n{}",
            entry.text.chars().take(1200).collect::<String>()
        ),
    )
}

pub fn mark_generated(board: &NSPasteboard) {
    board.setData_forType(
        Some(&NSData::new()),
        ns_string!("org.nspasteboard.AutoGeneratedType"),
    );
}
pub fn copy(text: &str) -> Result<(), String> {
    let board = NSPasteboard::generalPasteboard();
    board.clearContents();
    if !board.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString }) {
        return Err(tr!("无法写入剪贴板。", "Could not write to the clipboard.").into());
    }
    mark_generated(&board);
    Ok(())
}
