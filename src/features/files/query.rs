use super::{Entry, expand, ranked_matching, score_with_parent};
use crate::{tr, trf};
use regex::{Regex, RegexBuilder};
use regex_syntax::hir::literal::{ExtractKind, Extractor};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    All,
    Folders,
    Files,
    Documents,
    Images,
    Audio,
    Video,
    Archives,
}

impl Kind {
    pub const ALL: [Self; 8] = [
        Self::All,
        Self::Folders,
        Self::Files,
        Self::Documents,
        Self::Images,
        Self::Audio,
        Self::Video,
        Self::Archives,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => tr!("全部类型", "All types"),
            Self::Folders => tr!("目录", "Folders"),
            Self::Files => tr!("文件", "Files"),
            Self::Documents => tr!("文档", "Documents"),
            Self::Images => tr!("图片", "Images"),
            Self::Audio => tr!("音频", "Audio"),
            Self::Video => tr!("视频", "Video"),
            Self::Archives => tr!("压缩包", "Archives"),
        }
    }

    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Documents => &[
                "pdf", "txt", "md", "rtf", "doc", "docx", "pages", "xls", "xlsx", "numbers", "csv",
                "ppt", "pptx", "key", "odt",
            ],
            Self::Images => &[
                "png", "jpg", "jpeg", "gif", "heic", "heif", "webp", "svg", "tif", "tiff", "bmp",
                "avif", "ico",
            ],
            Self::Audio => &["mp3", "m4a", "wav", "flac", "aac", "aiff", "ogg", "opus"],
            Self::Video => &["mp4", "mov", "mkv", "webm", "avi", "m4v", "mpeg", "mpg"],
            Self::Archives => &["zip", "gz", "tar", "7z", "rar", "bz2", "xz", "zst", "tgz"],
            _ => &[],
        }
    }

    pub fn allows(self, entry: &Entry) -> bool {
        match self {
            Self::All => true,
            Self::Folders => entry.directory,
            Self::Files => !entry.directory,
            _ => {
                !entry.directory
                    && entry
                        .path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| {
                            self.extensions()
                                .iter()
                                .any(|ext| e.eq_ignore_ascii_case(ext))
                        })
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub kind: Kind,
    pub regex: bool,
}

pub struct Matcher {
    pub directory: Option<PathBuf>,
    pub term: String,
    pub options: Options,
    regex: Option<Regex>,
}

pub fn search_path(query: &str, home: &Path, regex: bool) -> Option<(PathBuf, String)> {
    if !regex {
        return super::path_query(query, home);
    }
    let query = query.trim();
    if query == "~" {
        return Some((home.into(), String::new()));
    }
    if !query.starts_with('/') && !query.starts_with("~/") {
        return None;
    }
    // A slash inside a character class (e.g. [^/]) belongs to the expression.
    let (mut class, mut escaped, mut slash) = (false, false, None);
    for (index, ch) in query.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '[' => class = true,
            ']' => class = false,
            '/' if !class => slash = Some(index),
            _ => {}
        }
    }
    let slash = slash?;
    Some((expand(&query[..=slash], home)?, query[slash + 1..].into()))
}

pub fn parent_search_path(query: &str, home: &Path, regex: bool) -> Option<String> {
    if regex
        && let Some((_, term)) = search_path(query, home, true)
        && !term.is_empty()
    {
        let query = query.trim();
        return Some(query[..query.len() - term.len()].to_owned());
    }
    super::parent_query(query)
}

impl Matcher {
    pub fn new(query: &str, home: &Path, options: Options) -> Result<Self, String> {
        let (directory, term) = match search_path(query, home, options.regex) {
            Some((directory, leaf)) => (Some(directory), leaf),
            None => (None, query.trim().to_owned()),
        };
        let regex = if options.regex && !term.is_empty() {
            if term.len() > 1024 {
                return Err(tr!(
                    "正则表达式过长，请缩短后重试。",
                    "Regular expression is too long. Shorten it and try again."
                )
                .into());
            }
            Some(
                RegexBuilder::new(&term)
                    .case_insensitive(true)
                    .size_limit(512 * 1024)
                    .dfa_size_limit(512 * 1024)
                    .build()
                    .map_err(|error| {
                        trf!(
                            "正则表达式无效：{}",
                            "Invalid regular expression: {}",
                            error.to_string().lines().last().unwrap_or("")
                        )
                    })?,
            )
        } else {
            None
        };
        Ok(Self {
            directory,
            term,
            options,
            regex,
        })
    }

    pub fn score(&self, entry: &Entry) -> Option<u8> {
        if !self.options.kind.allows(entry) {
            return None;
        }
        match &self.regex {
            Some(regex) => regex.is_match(&entry.name).then_some(0),
            None => score_with_parent(entry, &self.term, self.directory.is_none()),
        }
    }

    pub fn matching(&self, entries: &[Entry], recent: &[Entry]) -> Vec<Entry> {
        ranked_matching(entries, recent, |entry| self.score(entry))
    }

    /// Conservative Spotlight hints: every regex match contains at least one literal.
    pub fn literals(&self) -> Option<Vec<String>> {
        self.regex.as_ref()?;
        let hir = regex_syntax::Parser::new().parse(&self.term).ok()?;
        [ExtractKind::Prefix, ExtractKind::Suffix]
            .into_iter()
            .filter_map(|kind| {
                let sequence = Extractor::new().kind(kind).limit_total(16).extract(&hir);
                let literals = sequence.literals()?;
                if literals.is_empty() || literals.iter().any(|l| l.as_bytes().is_empty()) {
                    return None;
                }
                literals
                    .iter()
                    .map(|l| String::from_utf8(l.as_bytes().to_vec()).ok())
                    .collect::<Option<Vec<_>>>()
            })
            .max_by_key(|literals| literals.iter().map(String::len).min().unwrap_or(0))
    }
}
