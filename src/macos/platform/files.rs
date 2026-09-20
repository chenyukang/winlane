use crate::macos::platform::main_wake::WakeHandle;
use block2::RcBlock;
use objc2::rc::autoreleasepool;
use objc2::{msg_send, runtime::AnyObject};
use objc2_foundation::*;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, Receiver, Sender},
};
use std::time::{Duration, Instant};
use winlane::features::files::query::{Kind, Matcher, Options};
use winlane::features::files::{self, Entry, Settings};
use winlane::{tr, trf};

pub struct Request {
    pub generation: u64,
    pub query: String,
    pub options: Options,
    pub settings: Settings,
    pub recent: Vec<Entry>,
}
pub enum Update {
    Recent(Vec<Entry>),
    Results {
        generation: u64,
        entries: Vec<Entry>,
        gathering: bool,
        limited: bool,
        error: Option<String>,
    },
}
enum Work {
    Search(Request),
    Save(Vec<Entry>),
}
pub struct Service {
    sender: Sender<Work>,
    pub receiver: Receiver<Update>,
    generation: Arc<AtomicU64>,
}
impl Service {
    pub fn new(home: PathBuf, wake: WakeHandle) -> Self {
        let (sender, work) = mpsc::channel();
        let (updates, receiver) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let current = generation.clone();
        std::thread::spawn(move || {
            let path = files::history::path(&home);
            let _ = updates.send(Update::Recent(
                files::history::load(&path).unwrap_or_default(),
            ));
            wake.signal();
            while let Ok(work) = work.recv() {
                match work {
                    Work::Save(entries) => {
                        if files::history::save(&path, &entries).is_err() {
                            eprintln!("Could not save recent files");
                        }
                    }
                    Work::Search(request) => {
                        if current.load(Ordering::Acquire) != request.generation {
                            continue;
                        }
                        autoreleasepool(|_| search(request, &home, &current, &updates, &wake));
                    }
                }
            }
        });
        Self {
            sender,
            receiver,
            generation,
        }
    }
    pub fn cancel(&self) -> u64 {
        self.generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
    }
    pub fn search(&self, request: Request) {
        let _ = self.sender.send(Work::Search(request));
    }
    pub fn save(&self, recent: Vec<Entry>) {
        let _ = self.sender.send(Work::Save(recent));
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn search(
    request: Request,
    home: &std::path::Path,
    current: &AtomicU64,
    updates: &Sender<Update>,
    wake: &WakeHandle,
) {
    let cancelled = || current.load(Ordering::Acquire) != request.generation;
    let send = |entries, gathering, limited, error| {
        if !cancelled() {
            let _ = updates.send(Update::Results {
                generation: request.generation,
                entries,
                gathering,
                limited,
                error,
            });
            wake.signal();
        }
    };
    let matcher = match Matcher::new(&request.query, home, request.options) {
        Ok(matcher) => matcher,
        Err(error) => {
            send(Vec::new(), false, false, Some(error));
            return;
        }
    };
    if request.query.trim().is_empty() && request.options.kind == Kind::All {
        let mut entries: Vec<_> = request
            .recent
            .into_iter()
            .filter(|e| request.settings.allows(&e.path, home))
            .filter_map(|e| Entry::read(e.path).ok())
            .collect();
        if entries.is_empty() {
            entries = ["Desktop", "Documents", "Downloads", "Pictures"]
                .iter()
                .filter_map(|dir| Entry::read(home.join(dir)).ok())
                .filter(|e| request.settings.allows(&e.path, home))
                .collect();
        }
        send(entries, false, false, None);
        return;
    }
    if matcher.directory.is_some() {
        match files::browse_with(&matcher, cancelled) {
            Ok(entries) => {
                let limited = entries.len() >= files::MAX_CANDIDATES;
                send(
                    matcher.matching(&entries, &request.recent),
                    false,
                    limited,
                    None,
                );
            }
            Err(error) => send(Vec::new(), false, false, Some(error)),
        }
        return;
    }
    let predicate = search_predicate(&matcher);
    let query = NSMetadataQuery::new();
    query.setPredicate(Some(&predicate));
    query.setNotificationBatchingInterval(0.1);
    let roots: Vec<_> = request
        .settings
        .roots(home)
        .iter()
        .map(|p| NSString::from_str(&p.to_string_lossy()))
        .collect();
    // SAFETY: Spotlight accepts directory strings in its search-scope array.
    unsafe {
        let _: () = msg_send![&query, setSearchScopes: &*NSArray::from_retained_slice(&roots)];
    }
    let center = NSNotificationCenter::defaultCenter();
    let dirty = Arc::new(AtomicBool::new(true));
    let mut observers = Vec::new();
    for name in unsafe {
        [
            NSMetadataQueryGatheringProgressNotification,
            NSMetadataQueryDidFinishGatheringNotification,
            NSMetadataQueryDidUpdateNotification,
        ]
    } {
        let dirty = dirty.clone();
        let block = RcBlock::new(move |_: std::ptr::NonNull<NSNotification>| {
            dirty.store(true, Ordering::Release);
        });
        // SAFETY: The query and observers live on this worker's run loop; the block captures only an atomic flag.
        let observer = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(name),
                Some(&query),
                None,
                &block,
            )
        };
        observers.push(observer);
    }
    if !query.startQuery() {
        send(
            Vec::new(),
            false,
            false,
            Some(
                tr!(
                    "无法启动 Spotlight 搜索。请检查系统的 Spotlight 设置。",
                    "Could not start Spotlight search. Check Spotlight settings."
                )
                .into(),
            ),
        );
    } else {
        let started = Instant::now();
        let mut last = Instant::now() - Duration::from_secs(1);
        while !cancelled() {
            autoreleasepool(|_| {
                if dirty.load(Ordering::Acquire) && last.elapsed() >= Duration::from_millis(100) {
                    dirty.store(false, Ordering::Release);
                    query.disableUpdates();
                    let count = query.resultCount();
                    let deadline = Instant::now() + Duration::from_secs(2);
                    let mut scanned = 0;
                    let entries: Vec<_> = (0..count)
                        .take_while(|i| {
                            let read = !cancelled() && Instant::now() < deadline;
                            if read {
                                scanned = *i + 1;
                            }
                            read
                        })
                        .filter_map(|i| {
                            autoreleasepool(|_| {
                                let item =
                                    query.resultAtIndex(i).downcast::<NSMetadataItem>().ok()?;
                                let text = |key: &str| {
                                    item.valueForAttribute(&NSString::from_str(key))?
                                        .downcast::<NSString>()
                                        .ok()
                                        .map(|s| s.to_string())
                                };
                                let path = PathBuf::from(text("kMDItemPath")?);
                                if !request.settings.allows(&path, home) {
                                    return None;
                                }
                                let name = path.file_name()?.to_string_lossy().into_owned();
                                let directory = text("kMDItemContentType").is_some_and(|t| {
                                    t == "public.folder" || t == "public.directory"
                                });
                                let modified = item
                                    .valueForAttribute(ns_string!("kMDItemFSContentChangeDate"))
                                    .and_then(|d| d.downcast::<NSDate>().ok())
                                    .map_or(0, |d| d.timeIntervalSince1970().max(0.0) as u64);
                                Some(Entry {
                                    path,
                                    name,
                                    directory,
                                    modified,
                                })
                            })
                        })
                        .filter(|entry| matcher.score(entry).is_some())
                        .take(files::MAX_CANDIDATES)
                        .collect();
                    let gathering = query.isGathering();
                    query.enableUpdates();
                    send(
                        matcher.matching(&entries, &request.recent),
                        gathering,
                        scanned < count,
                        None,
                    );
                    last = Instant::now();
                }
                // A finite wait makes cancellation responsive even if Spotlight sends no events.
                let until = NSDate::dateWithTimeIntervalSinceNow(0.05);
                let handled = unsafe {
                    NSRunLoop::currentRunLoop().runMode_beforeDate(NSDefaultRunLoopMode, &until)
                };
                // A run loop with no sources returns immediately; do not spin while waiting.
                if !handled {
                    std::thread::sleep(Duration::from_millis(50));
                }
            });
            if query.isGathering() && started.elapsed() > Duration::from_secs(12) {
                send(
                    Vec::new(),
                    false,
                    false,
                    Some(
                        tr!(
                            "搜索尚未完成，请缩小范围或按 ⌘R 重试。",
                            "Search did not finish. Narrow the search or press ⌘R to retry."
                        )
                        .into(),
                    ),
                );
                break;
            }
        }
    }
    query.stopQuery();
    for observer in observers {
        unsafe {
            let _: () = msg_send![&center, removeObserver: &*observer];
        }
    }
}

pub(crate) fn predicate(text: &str) -> Option<objc2::rc::Retained<NSPredicate>> {
    let mut predicates = Vec::new();
    for word in text.split_whitespace().take(12) {
        let value = NSString::from_str(word);
        let arguments = NSArray::from_slice(&[&*value as &AnyObject]);
        // SAFETY: User input is a string argument, never part of the predicate format.
        predicates.push(unsafe {
            NSPredicate::predicateWithFormat_argumentArray(
                ns_string!("kMDItemFSName CONTAINS[cd] %@"),
                Some(&arguments),
            )
        });
        let length = word.chars().count();
        if (2..=40).contains(&length) && word.chars().all(char::is_alphanumeric) {
            let pattern = format!(
                "*{}*",
                word.chars()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join("*")
            );
            let value = NSString::from_str(&pattern);
            let arguments = NSArray::from_slice(&[&*value as &AnyObject]);
            // Keep short abbreviations useful for folders without collecting
            // every file that happens to contain the same two letters.
            let format = if length == 2 {
                ns_string!(
                    "kMDItemFSName LIKE[cd] %@ AND (kMDItemContentType == 'public.folder' OR kMDItemContentType == 'public.directory')"
                )
            } else {
                ns_string!("kMDItemFSName LIKE[cd] %@")
            };
            predicates.push(unsafe {
                NSPredicate::predicateWithFormat_argumentArray(format, Some(&arguments))
            });
        }
    }
    if predicates.len() <= 1 {
        // Spotlight rejects an OR group with only one condition (short queries).
        predicates.pop()
    } else {
        Some(
            NSCompoundPredicate::orPredicateWithSubpredicates(&NSArray::from_retained_slice(
                &predicates,
            ))
            .into_super(),
        )
    }
}

pub(crate) fn search_predicate(matcher: &Matcher) -> objc2::rc::Retained<NSPredicate> {
    let mut conditions = Vec::new();
    if matcher.options.regex {
        if let Some(literals) = matcher.literals() {
            let predicates: Vec<_> = literals
                .iter()
                .map(|literal| unsafe {
                    NSPredicate::predicateWithFormat_argumentArray(
                        ns_string!("kMDItemFSName CONTAINS[cd] %@"),
                        Some(&NSArray::from_slice(&[
                            &*NSString::from_str(literal) as &AnyObject
                        ])),
                    )
                })
                .collect();
            conditions.push(if predicates.len() == 1 {
                predicates[0].clone()
            } else {
                NSCompoundPredicate::orPredicateWithSubpredicates(&NSArray::from_retained_slice(
                    &predicates,
                ))
                .into_super()
            });
        }
    } else if let Some(predicate) = predicate(&matcher.term) {
        conditions.push(predicate);
    }
    let kind = matcher.options.kind;
    if kind != Kind::All {
        let directory =
            "(kMDItemContentType == 'public.folder' OR kMDItemContentType == 'public.directory')";
        let format = match kind {
            Kind::Folders => directory.into(),
            Kind::Files => format!("NOT {directory}"),
            _ => format!(
                "NOT {directory} AND ({})",
                kind.extensions()
                    .iter()
                    .map(|ext| format!("kMDItemFSName ENDSWITH[cd] '.{ext}'"))
                    .collect::<Vec<_>>()
                    .join(" OR ")
            ),
        };
        conditions.push(unsafe {
            NSPredicate::predicateWithFormat_argumentArray(&NSString::from_str(&format), None)
        });
    }
    match conditions.len() {
        0 => unsafe {
            NSPredicate::predicateWithFormat_argumentArray(
                ns_string!("kMDItemFSName LIKE '*'"),
                None,
            )
        },
        1 => conditions.pop().unwrap(),
        _ => NSCompoundPredicate::andPredicateWithSubpredicates(&NSArray::from_retained_slice(
            &conditions,
        ))
        .into_super(),
    }
}

pub fn file_url(path: &std::path::Path) -> Result<objc2::rc::Retained<NSURL>, String> {
    if !path.is_absolute() {
        return Err(tr!("文件路径无效。", "Invalid file path.").into());
    }
    Ok(NSURL::fileURLWithPath(&NSString::from_str(
        &path.to_string_lossy(),
    )))
}

pub fn open(path: &std::path::Path, wake: WakeHandle) -> Receiver<Result<i32, String>> {
    use objc2_app_kit::{NSRunningApplication, NSWorkspace, NSWorkspaceOpenConfiguration};
    let (tx, rx) = mpsc::channel();
    let url = file_url(path).expect("file search only supplies absolute paths");
    let config = NSWorkspaceOpenConfiguration::configuration();
    config.setActivates(true);
    let done = RcBlock::new(move |app: *mut NSRunningApplication, error: *mut NSError| {
        let result = unsafe {
            if let Some(error) = error.as_ref() {
                Err(trf!(
                    "无法打开文件：{}",
                    "Could not open file: {}",
                    error.localizedDescription()
                ))
            } else {
                Ok(app.as_ref().map_or(0, |a| a.processIdentifier()))
            }
        };
        let _ = tx.send(result);
        wake.signal();
    });
    NSWorkspace::sharedWorkspace().openURL_configuration_completionHandler(
        &url,
        &config,
        Some(&done),
    );
    rx
}
