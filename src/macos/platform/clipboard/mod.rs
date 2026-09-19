use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_app_kit::{NSImage, NSPasteboard, NSPasteboardTypeString, NSWorkspace};
use objc2_foundation::{
    NSData, NSDate, NSDateFormatter, NSDateFormatterStyle, NSHomeDirectory, NSString, ns_string,
};
use std::path::PathBuf;
use winlane::features::clipboard::store::Store;
use winlane::features::clipboard::{
    ClipboardSettings, Entry, History, ImageFormat, ImageInfo, MAX_IMAGE_BYTES, ignored,
};
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
    pub fn read(
        &mut self,
        board: &NSPasteboard,
        settings: &ClipboardSettings,
        source_id: &str,
    ) -> Option<Capture> {
        let count = board.changeCount();
        if count == self.count {
            return None;
        }
        self.count = count;
        if !settings.enabled {
            return None;
        }
        let types: Vec<_> = board
            .types()
            .map(|types| types.iter().map(|kind| kind.to_string()).collect())
            .unwrap_or_default();
        if ignored(&types, source_id) {
            return None;
        }
        for format in [ImageFormat::Png, ImageFormat::Tiff] {
            if types.iter().any(|kind| kind == format.pasteboard_type())
                && let Some(data) = board.dataForType(&NSString::from_str(format.pasteboard_type()))
            {
                if data.length() > MAX_IMAGE_BYTES || data.length() == 0 {
                    return None;
                }
                let bytes = data.to_vec();
                return (board.changeCount() == count).then_some(Capture::Image(bytes, format));
            }
        }
        if types.iter().any(|kind| kind == "public.file-url") {
            return None;
        }
        let text = board.stringForType(unsafe { NSPasteboardTypeString })?;
        if text.length() > winlane::features::clipboard::MAX_ITEM_BYTES
            || board.changeCount() != count
        {
            return None;
        }
        Some(Capture::Text(text.to_string()))
    }
}

pub enum Capture {
    Text(String),
    Image(Vec<u8>, ImageFormat),
}
struct PendingImage {
    receiver: std::sync::mpsc::Receiver<Result<ImageInfo, String>>,
    source: String,
    time: u64,
    generation: u64,
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
    image: Option<PendingImage>,
    generation: u64,
    thumbnails: std::cell::RefCell<std::collections::HashMap<String, Retained<NSImage>>>,
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
            image: None,
            generation: 0,
            thumbnails: Default::default(),
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
                    "历史无法持久保存，当前记录仅在本次运行期间保留。",
                    "History could not be saved. Current entries remain available for this session."
                )
                .into(),
            );
            changed = true;
        }
        if let Some(result) =
            self.image
                .as_ref()
                .and_then(|pending| match pending.receiver.try_recv() {
                    Ok(result) => Some(result),
                    Err(std::sync::mpsc::TryRecvError::Empty) => None,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(tr!(
                        "图片处理未完成，请重新复制。",
                        "Image processing did not finish. Copy it again."
                    )
                    .into())),
                })
        {
            let pending = self.image.take().unwrap();
            if pending.generation == self.generation {
                match result {
                    Ok(image) => {
                        self.history.record_image(
                            image,
                            &pending.source,
                            pending.time,
                            &self.settings,
                        );
                        self.persist();
                    }
                    Err(error) => self.error = Some(error),
                }
                changed = true;
            }
        }
        if self.history.prune(now(), &self.settings) {
            self.persist();
            changed = true;
        }
        changed
    }
    pub fn poll_clipboard(&mut self) -> bool {
        let mut changed = self.poll_storage();
        if self.loading
            || self.image.is_some()
            || !self.settings.enabled
            || !self.observer.changed(&self.board)
        {
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
        if let Some(capture) = self.observer.read(&self.board, &self.settings, &id) {
            changed |= self.accept(capture, &name, now());
        }
        changed
    }
    pub fn accept(&mut self, capture: Capture, source: &str, time: u64) -> bool {
        if !self.settings.enabled || self.image.is_some() {
            return false;
        }
        match capture {
            Capture::Text(text) => {
                let changed = self.history.record(&text, source, time, &self.settings);
                if changed {
                    self.persist();
                }
                changed
            }
            Capture::Image(bytes, format) => {
                let (sender, receiver) = std::sync::mpsc::channel();
                let files = self.store.files.clone();
                std::thread::spawn(move || {
                    let _ = sender.send(crate::macos::platform::clipboard::image::prepare(
                        bytes, format, &files,
                    ));
                });
                self.image = Some(PendingImage {
                    receiver,
                    source: source.into(),
                    time,
                    generation: self.generation,
                });
                false
            }
        }
    }
    pub fn thumbnail(&self, image: &ImageInfo) -> Option<Retained<NSImage>> {
        if let Some(value) = self.thumbnails.borrow().get(&image.key) {
            return Some(value.clone());
        }
        let path = &image.asset.as_ref()?.thumbnail;
        let data = std::fs::read(path).ok()?;
        if data.len() > 128 * 1024 {
            return None;
        }
        let value = NSImage::initWithData(NSImage::alloc(), &NSData::from_vec(data))?;
        let mut cache = self.thumbnails.borrow_mut();
        if cache.len() >= 32 {
            cache.clear();
        }
        cache.insert(image.key.clone(), value.clone());
        Some(value)
    }
    pub fn configure(&mut self, settings: ClipboardSettings) {
        if self.settings == settings {
            return;
        }
        self.observer.reset(&self.board);
        if self.settings.enabled && !settings.enabled {
            self.generation = self.generation.wrapping_add(1);
        }
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
        self.generation = self.generation.wrapping_add(1);
        self.thumbnails.borrow_mut().clear();
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
            if entry.image.is_some() {
                entry.preview()
            } else {
                entry.text.chars().take(1200).collect::<String>()
            }
        ),
    )
}

pub fn mark_generated(board: &NSPasteboard) {
    board.setData_forType(
        Some(&NSData::new()),
        ns_string!("org.nspasteboard.AutoGeneratedType"),
    );
}
pub enum PasteContent {
    Text(String),
    Image(ImageFormat, Vec<u8>),
}
impl PasteContent {
    pub fn from_entry(entry: &Entry) -> Result<Self, String> {
        if let Some(image) = &entry.image {
            let bytes = winlane::features::clipboard::assets::image_bytes(image).map_err(|_| {
                tr!(
                    "图片文件无法读取，请重新复制。",
                    "The image file could not be read. Copy the image again."
                )
            })?;
            Ok(Self::Image(image.format, bytes))
        } else {
            Ok(Self::Text(entry.text.to_string()))
        }
    }
    pub fn write(&self, board: &NSPasteboard) -> Result<(), String> {
        board.clearContents();
        let written = match self {
            Self::Text(text) => board
                .setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString }),
            Self::Image(format, bytes) => board.setData_forType(
                Some(&NSData::with_bytes(bytes)),
                &NSString::from_str(format.pasteboard_type()),
            ),
        };
        if !written {
            return Err(tr!("无法写入剪贴板。", "Could not write to the clipboard.").into());
        }
        mark_generated(board);
        Ok(())
    }
}

pub(crate) mod image;
