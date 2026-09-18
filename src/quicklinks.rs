use crate::{snippets, tr, trf};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

pub const MAX_LINKS: usize = 200;
pub const MAX_LINK_BYTES: usize = 16 * 1024;
pub const MAX_IMPORT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quicklink {
    pub id: String,
    pub name: String,
    pub link: String,
    #[serde(default, rename = "openWith")]
    pub open_with: String,
}

impl Quicklink {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() || self.id.len() > 100 {
            return Err(tr!("快捷链接标识无效。", "Invalid quicklink ID.").into());
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 120 {
            return Err(tr!(
                "请输入名称，最多 120 个字符。",
                "Enter a name, up to 120 characters."
            )
            .into());
        }
        if self.open_with.len() > 1024 || self.open_with.chars().any(char::is_control) {
            return Err(tr!("打开方式无效。", "Invalid target application.").into());
        }
        let template = Template::parse(&self.link)?;
        let values = template
            .arguments
            .iter()
            .map(|a| {
                (
                    a.name.clone(),
                    a.default.clone().unwrap_or_else(|| match &a.kind {
                        snippets::ArgumentKind::Choice(options) => options[0].clone(),
                        _ => "value".into(),
                    }),
                )
            })
            .collect();
        template.render("https://example.invalid/value", &values, |_, _| {
            "2026-01-01".into()
        })?;
        Ok(())
    }
}

pub fn validate(links: &[Quicklink]) -> Result<(), String> {
    if links.len() > MAX_LINKS {
        return Err(tr!(
            "最多保存 200 个快捷链接。",
            "You can save up to 200 quicklinks."
        )
        .into());
    }
    let mut ids = HashSet::new();
    for link in links {
        link.validate()?;
        if !ids.insert(&link.id) {
            return Err(tr!("快捷链接标识重复。", "Quicklink IDs must be unique.").into());
        }
    }
    Ok(())
}

pub fn matching(links: &[Quicklink], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    let mut matches: Vec<_> = links
        .iter()
        .enumerate()
        .filter_map(|(index, link)| {
            let name = link.name.to_lowercase();
            let address = link.link.to_lowercase();
            query
                .split_whitespace()
                .all(|term| name.contains(term) || address.contains(term))
                .then_some((
                    if name == query {
                        0
                    } else if name.starts_with(&query) {
                        1
                    } else if name.contains(&query) {
                        2
                    } else {
                        3
                    },
                    index,
                ))
        })
        .collect();
    matches.sort_by_key(|&(rank, index)| (rank, index));
    matches.into_iter().map(|(_, index)| index).collect()
}

#[derive(Clone, Debug)]
enum Part {
    Literal(String),
    Value(snippets::Template, bool),
}

#[derive(Clone, Debug)]
pub struct Template {
    parts: Vec<Part>,
    pub arguments: Vec<snippets::Argument>,
    file: bool,
}

impl Template {
    pub fn parse(link: &str) -> Result<Self, String> {
        if link.trim().is_empty()
            || link.len() > MAX_LINK_BYTES
            || link.chars().any(char::is_control)
        {
            return Err(tr!(
                "请输入链接或绝对路径，最多 16 KB。",
                "Enter a link or absolute path, up to 16 KB."
            )
            .into());
        }
        let link = link.trim();
        let mut template = Self {
            parts: Vec::new(),
            arguments: Vec::new(),
            file: link.starts_with('/') || link.starts_with("~/"),
        };
        let mut rest = link;
        let mut literal = String::new();
        let mut unnamed = 0;
        while !rest.is_empty() {
            if let Some(after) = rest.strip_prefix("\\{") {
                literal.push('{');
                rest = after;
                continue;
            }
            if let Some(after) = rest.strip_prefix('{') {
                let end = token_end(after).ok_or_else(invalid_placeholder)?;
                let token = after[..end].trim();
                let (token, raw) = match token.rsplit_once('|') {
                    Some((value, modifier)) if modifier.trim() == "raw" => (value.trim(), true),
                    Some(_) => return Err(invalid_placeholder()),
                    None => (token, false),
                };
                let token = if token == "argument" {
                    unnamed += 1;
                    format!("argument name=\"{} {}\"", tr!("输入", "Value"), unnamed)
                } else if matches!(
                    token,
                    "clipboard" | "date" | "time" | "datetime" | "timestamp"
                ) || token.starts_with("argument ")
                    || token.starts_with("date ")
                    || token.starts_with("time ")
                    || token.starts_with("datetime ")
                {
                    token.to_owned()
                } else if !token.is_empty() && !token.contains(['=', '{', '}', '"']) {
                    format!("argument name={}", serde_json::to_string(token).unwrap())
                } else {
                    return Err(invalid_placeholder());
                };
                let value = snippets::Template::parse(&format!("{{{token}}}"))?;
                for argument in &value.arguments {
                    if let Some(previous) =
                        template.arguments.iter().find(|a| a.name == argument.name)
                    {
                        if previous != argument {
                            return Err(invalid_placeholder());
                        }
                    } else {
                        template.arguments.push(argument.clone());
                    }
                }
                if template.arguments.len() > 8 {
                    return Err(tr!("最多使用 8 个输入字段。", "Use up to 8 input fields.").into());
                }
                if !literal.is_empty() {
                    template
                        .parts
                        .push(Part::Literal(std::mem::take(&mut literal)));
                }
                template.parts.push(Part::Value(value, raw));
                rest = &after[end + 1..];
            } else {
                let ch = rest.chars().next().unwrap();
                literal.push(ch);
                rest = &rest[ch.len_utf8()..];
            }
        }
        if !literal.is_empty() {
            template.parts.push(Part::Literal(literal));
        }
        Ok(template)
    }

    pub fn uses_clipboard(&self) -> bool {
        self.parts
            .iter()
            .any(|part| matches!(part, Part::Value(value, _) if value.uses_clipboard()))
    }

    pub fn render(
        &self,
        clipboard: &str,
        values: &HashMap<String, String>,
        mut date: impl FnMut(&str, Option<&str>) -> String,
    ) -> Result<String, String> {
        let mut result = String::new();
        for part in &self.parts {
            let text = match part {
                Part::Literal(text) => text.clone(),
                Part::Value(template, raw) => {
                    let value = template.render(clipboard, values, &mut date)?;
                    if *raw || self.file {
                        value
                    } else {
                        percent_encode(&value)
                    }
                }
            };
            if result.len().saturating_add(text.len()) > MAX_LINK_BYTES {
                return Err(tr!("展开后的链接超过 16 KB。", "Expanded link exceeds 16 KB.").into());
            }
            result.push_str(&text);
        }
        destination(&result)
    }
}

fn token_end(text: &str) -> Option<usize> {
    let (mut quoted, mut escaped) = (false, false);
    for (i, ch) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted && ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            quoted = !quoted;
        } else if ch == '}' && !quoted {
            return Some(i);
        }
    }
    None
}

fn invalid_placeholder() -> String {
    tr!(
        "占位符无效。可使用 {Query}、{argument name=\"Query\"}、{clipboard}，或添加 | raw。",
        "Invalid placeholder. Use {Query}, {argument name=\"Query\"}, {clipboard}, or add | raw."
    )
    .into()
}

fn percent_encode(text: &str) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            output.push(byte as char);
        } else {
            write!(output, "%{byte:02X}").unwrap();
        }
    }
    output
}

pub fn destination(value: &str) -> Result<String, String> {
    let value = value.trim();
    let invalid = || {
        tr!(
            "请输入有效的网址、应用链接或绝对路径。",
            "Enter a valid URL, app link, or absolute path."
        )
        .to_owned()
    };
    if value.is_empty() || value.len() > MAX_LINK_BYTES || value.chars().any(char::is_control) {
        return Err(invalid());
    }
    if value.starts_with('/') || value.starts_with("~/") {
        return Ok(value.into());
    }
    if let Some((scheme, remainder)) = value.split_once(':') {
        let scheme = scheme.to_ascii_lowercase();
        if scheme.is_empty()
            || !scheme.as_bytes()[0].is_ascii_alphabetic()
            || !scheme
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"+.-".contains(&c))
            || remainder.is_empty()
            || matches!(
                scheme.as_str(),
                "javascript" | "data" | "vbscript" | "shell"
            )
        {
            return Err(invalid());
        }
        if matches!(scheme.as_str(), "https" | "http")
            && !remainder.strip_prefix("//").is_some_and(|s| {
                !s.is_empty()
                    && !s.starts_with('/')
                    && !s
                        .split('/')
                        .next()
                        .unwrap_or("")
                        .contains(char::is_whitespace)
            })
        {
            return Err(invalid());
        }
        Ok(value.into())
    } else if value
        .split('/')
        .next()
        .is_some_and(|host| host.contains('.') && !host.contains(char::is_whitespace))
    {
        Ok(format!("https://{value}"))
    } else {
        Err(invalid())
    }
}

#[derive(Deserialize)]
struct ExportedLink {
    name: String,
    link: String,
    #[serde(default, rename = "openWith")]
    open_with: Option<String>,
}

pub struct Import {
    pub links: Vec<Quicklink>,
    pub added: usize,
    pub skipped: usize,
}

pub fn import_json(existing: &[Quicklink], json: &str) -> Result<Import, String> {
    if json.len() > MAX_IMPORT_BYTES {
        return Err(tr!("导入文件超过 4 MB。", "Import file exceeds 4 MB.").into());
    }
    let entries: Vec<ExportedLink> = serde_json::from_str(json).map_err(|_| {
        tr!(
            "请选择 Raycast 导出的 Quicklinks JSON 文件。",
            "Choose a Quicklinks JSON export from Raycast."
        )
        .to_owned()
    })?;
    if entries.len() > MAX_LINKS {
        return Err(tr!(
            "导入文件超过 200 个链接。",
            "Import contains more than 200 links."
        )
        .into());
    }
    let mut links = existing.to_vec();
    let (mut added, mut skipped) = (0, 0);
    for (index, entry) in entries.into_iter().enumerate() {
        let name = entry.name.trim().to_owned();
        let link = entry.link.trim().to_owned();
        let open_with = entry.open_with.unwrap_or_default().trim().to_owned();
        if links
            .iter()
            .any(|q| q.name == name && q.link == link && q.open_with == open_with)
        {
            skipped += 1;
            continue;
        }
        let identity = serde_json::to_vec(&(&name, &link, &open_with)).unwrap();
        let mut id = format!("import-{:x}", Sha256::digest(&identity));
        while links.iter().any(|q| q.id == id) {
            id.push('x');
        }
        let candidate = Quicklink {
            id,
            name,
            link,
            open_with,
        };
        candidate.validate().map_err(|e| {
            trf!(
                "第 {} 个链接无效：{}",
                "Link {} is invalid: {}",
                index + 1,
                e
            )
        })?;
        links.push(candidate);
        added += 1;
    }
    validate(&links)?;
    Ok(Import {
        links,
        added,
        skipped,
    })
}
