use super::{Entry, expand, ranked_matching, score_with_parent};
use crate::{tr, trf};
use regex::{Regex, RegexBuilder};
use regex_syntax::hir::literal::{ExtractKind, Extractor};
use std::path::{Path, PathBuf};

pub struct Matcher {
    pub directory: Option<PathBuf>,
    pub term: String,
    pub error: Option<String>,
    pattern: bool,
    expression: Option<String>,
    regex: Option<Regex>,
}

/// Regex-specific operators take precedence over shell-style wildcards.
fn expression(term: &str) -> Option<String> {
    if term.contains(['\\', '^', '$', '(', ')', '{', '}', '|', '+']) || term.contains(".*") {
        Some(term.to_owned())
    } else if term.contains(['*', '?']) {
        let mut expression = String::from("^");
        let mut class = false;
        for ch in term.chars() {
            match ch {
                '[' => {
                    class = true;
                    expression.push(ch);
                }
                ']' => {
                    class = false;
                    expression.push(ch);
                }
                '*' if !class => expression.push_str(".*"),
                '?' if !class => expression.push('.'),
                _ if class => expression.push(ch),
                _ => expression.push_str(&regex::escape(&ch.to_string())),
            }
        }
        expression.push('$');
        Some(expression)
    } else if term.contains(['[', ']']) {
        Some(term.to_owned())
    } else {
        None
    }
}

pub fn search_path(query: &str, home: &Path) -> Option<(PathBuf, String)> {
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

pub fn parent_search_path(query: &str, home: &Path) -> Option<String> {
    if let Some((_, term)) = search_path(query, home)
        && !term.is_empty()
    {
        let query = query.trim();
        return Some(query[..query.len() - term.len()].to_owned());
    }
    super::parent_query(query)
}

impl Matcher {
    /// Parse without compiling a pattern or accessing disk; safe for cached UI filtering.
    pub fn literal(query: &str, home: &Path) -> Self {
        let (directory, term) = match search_path(query, home) {
            Some((directory, leaf)) => (Some(directory), leaf),
            None => (None, query.trim().to_owned()),
        };
        let pattern = term.contains([
            '*', '?', '[', ']', '\\', '^', '$', '(', ')', '{', '}', '|', '+',
        ]);
        Self {
            directory,
            term,
            error: None,
            pattern,
            expression: None,
            regex: None,
        }
    }

    pub fn new(query: &str, home: &Path) -> Self {
        let mut matcher = Self::literal(query, home);
        if matcher.pattern {
            if matcher.term.len() > 1024 {
                matcher.error = Some(
                    tr!(
                        "表达式过长，仅按普通名称查找。",
                        "Pattern is too long; searching literal names only."
                    )
                    .into(),
                );
                return matcher;
            }
            matcher.expression = expression(&matcher.term);
            if let Some(expression) = &matcher.expression {
                match RegexBuilder::new(expression)
                    .case_insensitive(true)
                    .size_limit(512 * 1024)
                    .dfa_size_limit(512 * 1024)
                    .build()
                {
                    Ok(regex) => matcher.regex = Some(regex),
                    Err(error) => {
                        matcher.error = Some(trf!(
                            "表达式无效，仅按普通名称查找：{}",
                            "Invalid pattern; searching literal names only: {}",
                            error.to_string().lines().last().unwrap_or("")
                        ))
                    }
                }
            }
        }
        matcher
    }

    pub fn is_pattern(&self) -> bool {
        self.pattern
    }

    pub fn recursive(&self) -> bool {
        self.directory.is_some() && self.regex.is_some()
    }

    pub fn score(&self, entry: &Entry) -> Option<u8> {
        score_with_parent(entry, &self.term, self.directory.is_none())
            .or_else(|| self.regex.as_ref()?.is_match(&entry.name).then_some(5))
    }

    pub fn matching(&self, entries: &[Entry], recent: &[Entry]) -> Vec<Entry> {
        ranked_matching(entries, recent, |entry| self.score(entry))
    }

    /// Conservative Spotlight hints: every regex match contains at least one literal.
    pub fn literals(&self) -> Option<Vec<String>> {
        self.regex.as_ref()?;
        let hir = regex_syntax::Parser::new()
            .parse(self.expression.as_deref()?)
            .ok()?;
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
