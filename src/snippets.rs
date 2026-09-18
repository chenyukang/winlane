use crate::{tr, trf};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const MAX_BODY_BYTES: usize = 64 * 1024;
pub const MAX_RENDERED_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snippet {
    pub id: String,
    pub name: String,
    pub body: String,
}

impl Snippet {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() || self.id.len() > 100 {
            return Err(tr!("片段标识无效。", "Invalid snippet ID.").into());
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 120 {
            return Err(tr!(
                "请输入名称，最多 120 个字符。",
                "Enter a name, up to 120 characters."
            )
            .into());
        }
        if self.body.is_empty() || self.body.len() > MAX_BODY_BYTES {
            return Err(tr!(
                "请输入正文，最多 64 KB。",
                "Enter snippet text, up to 64 KB."
            )
            .into());
        }
        Template::parse(&self.body)?;
        Ok(())
    }
}

pub fn validate(snippets: &[Snippet]) -> Result<(), String> {
    if snippets.len() > 200 {
        return Err(tr!("最多保存 200 个片段。", "You can save up to 200 snippets.").into());
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for snippet in snippets {
        snippet.validate()?;
        if !ids.insert(&snippet.id) || !names.insert(snippet.name.trim().to_lowercase()) {
            return Err(tr!(
                "片段名称或标识重复。",
                "Snippet names and IDs must be unique."
            )
            .into());
        }
    }
    Ok(())
}

pub fn matching(snippets: &[Snippet], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let terms: Vec<_> = query.split_whitespace().collect();
    let mut matches: Vec<_> = snippets
        .iter()
        .enumerate()
        .filter_map(|(index, snippet)| {
            let name = snippet.name.to_lowercase();
            let body = snippet.body.to_lowercase();
            if !terms
                .iter()
                .all(|term| name.contains(term) || body.contains(term))
            {
                return None;
            }
            let rank = if name == query {
                0
            } else if name.starts_with(&query) {
                1
            } else if terms.iter().all(|term| name.contains(term)) {
                2
            } else {
                3
            };
            Some((rank, index))
        })
        .collect();
    matches.sort_by_key(|&(rank, index)| (rank, index));
    matches.into_iter().map(|(_, index)| index).collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArgumentKind {
    Text,
    Multiline,
    Choice(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Argument {
    pub name: String,
    pub default: Option<String>,
    pub required: bool,
    pub kind: ArgumentKind,
}

impl Argument {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty()
            || self.name.chars().count() > 80
            || self.name.contains(['\n', '\r'])
        {
            return Err(tr!(
                "请输入字段名称，最多 80 个字符。",
                "Enter a field name, up to 80 characters."
            )
            .into());
        }
        if let ArgumentKind::Choice(options) = &self.kind {
            let unique: HashSet<_> = options.iter().collect();
            if options.is_empty()
                || options.len() > 30
                || unique.len() != options.len()
                || options.iter().any(|option| {
                    option.trim().is_empty()
                        || option.contains(['\n', '\r'])
                        || option.chars().count() > 120
                })
            {
                return Err(tr!(
                    "请填写 1–30 个不同的选项，每行一个，最多 120 个字符。",
                    "Enter 1–30 unique options, one per line, up to 120 characters each."
                )
                .into());
            }
        }
        if let Some(value) = &self.default {
            self.validate_value(value, false)?;
        }
        Ok(())
    }

    fn validate_value(&self, value: &str, required: bool) -> Result<(), String> {
        if value.trim().is_empty() {
            return if required {
                Err(trf!("请填写 {}。", "Enter {}.", self.name))
            } else {
                Ok(())
            };
        }
        match &self.kind {
            ArgumentKind::Text if value.contains(['\n', '\r']) => Err(trf!(
                "{} 只接受单行文字。",
                "{} accepts single-line text only.",
                self.name
            )),
            ArgumentKind::Choice(options) if !options.iter().any(|option| option == value) => {
                Err(trf!(
                    "请为 {} 选择列表中的值。",
                    "Choose a listed value for {}.",
                    self.name
                ))
            }
            _ => Ok(()),
        }
    }

    pub fn placeholder(&self) -> Result<String, String> {
        self.validate()?;
        let mut token = format!(
            "{{argument name={} required=\"{}\"",
            quoted(&self.name),
            self.required
        );
        match &self.kind {
            ArgumentKind::Text => token.push_str(" type=\"text\""),
            ArgumentKind::Multiline => token.push_str(" type=\"multiline\""),
            ArgumentKind::Choice(options) => token.push_str(&format!(
                " type=\"choice\" options={}",
                quoted(&options.join("\n"))
            )),
        }
        if let Some(default) = &self.default {
            token.push_str(&format!(" default={}", quoted(default)));
        }
        token.push('}');
        Ok(token)
    }

    fn from_attributes(attrs: &HashMap<&str, String>) -> Result<Self, String> {
        if attrs
            .keys()
            .any(|key| !matches!(*key, "name" | "default" | "type" | "required" | "options"))
        {
            return Err(invalid_placeholder());
        }
        let default = attrs.get("default").cloned();
        let required = match attrs.get("required").map(String::as_str) {
            Some("true") => true,
            Some("false") => false,
            None => default.is_none(),
            _ => return Err(invalid_placeholder()),
        };
        let kind = match attrs.get("type").map(String::as_str) {
            None if !attrs.contains_key("options")
                && default
                    .as_ref()
                    .is_some_and(|value| value.contains(['\n', '\r'])) =>
            {
                ArgumentKind::Multiline
            }
            None | Some("text") if !attrs.contains_key("options") => ArgumentKind::Text,
            Some("multiline") if !attrs.contains_key("options") => ArgumentKind::Multiline,
            Some("choice") => ArgumentKind::Choice(
                attrs
                    .get("options")
                    .ok_or_else(invalid_placeholder)?
                    .split('\n')
                    .map(str::to_owned)
                    .collect(),
            ),
            _ => return Err(invalid_placeholder()),
        };
        let argument = Self {
            name: attrs
                .get("name")
                .cloned()
                .unwrap_or_else(|| tr!("输入", "Value").into()),
            default,
            required,
            kind,
        };
        argument.validate()?;
        Ok(argument)
    }

    pub fn preview_value(&self) -> String {
        self.default.clone().unwrap_or_else(|| match &self.kind {
            ArgumentKind::Choice(options) => options.first().cloned().unwrap_or_default(),
            _ => format!("⟨{}⟩", self.name),
        })
    }
}

fn quoted(value: &str) -> String {
    serde_json::to_string(value).expect("strings are JSON serializable")
}

pub struct DatePreset {
    pub title_zh: &'static str,
    pub title_en: &'static str,
    pub token: &'static str,
}

pub const DATE_PRESETS: &[DatePreset] = &[
    DatePreset {
        title_zh: "系统日期",
        title_en: "System date",
        token: "{date}",
    },
    DatePreset {
        title_zh: "日期",
        title_en: "Date",
        token: "{date format=\"yyyy-MM-dd\"}",
    },
    DatePreset {
        title_zh: "中文日期",
        title_en: "Chinese date",
        token: "{date format=\"yyyy年M月d日\"}",
    },
    DatePreset {
        title_zh: "日期与时间",
        title_en: "Date & time",
        token: "{date format=\"yyyy-MM-dd HH:mm\"}",
    },
    DatePreset {
        title_zh: "24 小时时间",
        title_en: "24-hour time",
        token: "{time format=\"HH:mm\"}",
    },
    DatePreset {
        title_zh: "星期",
        title_en: "Weekday",
        token: "{date format=\"EEEE\"}",
    },
    DatePreset {
        title_zh: "Unix 时间戳（秒）",
        title_en: "Unix timestamp (seconds)",
        token: "{timestamp}",
    },
];

pub fn date_placeholder(format: &str) -> Result<String, String> {
    if format.trim().is_empty() || format.chars().count() > 120 || format.contains(['\n', '\r']) {
        return Err(tr!(
            "请输入日期格式，最多 120 个字符。",
            "Enter a date format, up to 120 characters."
        )
        .into());
    }
    Ok(format!("{{date format={}}}", quoted(format)))
}

pub fn inserting(
    body: &str,
    location: usize,
    length: usize,
    token: &str,
) -> Result<String, String> {
    let mut units: Vec<_> = body.encode_utf16().collect();
    let end = location
        .checked_add(length)
        .filter(|end| *end <= units.len())
        .ok_or_else(invalid_placeholder)?;
    units.splice(location..end, token.encode_utf16());
    let candidate = String::from_utf16(&units).map_err(|_| invalid_placeholder())?;
    Template::parse(&candidate)?;
    Ok(candidate)
}

#[derive(Clone, Debug)]
enum Part {
    Literal(String),
    Clipboard,
    Date {
        kind: String,
        format: Option<String>,
    },
    Argument(usize),
}

#[derive(Clone, Debug)]
pub struct Template {
    parts: Vec<Part>,
    pub arguments: Vec<Argument>,
}

impl Template {
    pub fn parse(body: &str) -> Result<Self, String> {
        if body.len() > MAX_BODY_BYTES {
            return Err(tr!("片段正文过长。", "Snippet text is too long.").into());
        }
        let mut template = Self {
            parts: Vec::new(),
            arguments: Vec::new(),
        };
        let mut rest = body;
        let mut literal = String::new();
        while !rest.is_empty() {
            if let Some(after) = rest.strip_prefix("\\{") {
                literal.push('{');
                rest = after;
                continue;
            }
            if let Some(after) = rest.strip_prefix('{') {
                let kind_end = after
                    .find(|ch: char| ch.is_whitespace() || ch == '}' || ch == '{')
                    .unwrap_or(after.len());
                let recognized = matches!(
                    &after[..kind_end],
                    "clipboard" | "date" | "time" | "datetime" | "timestamp" | "argument"
                );
                if recognized {
                    let end = placeholder_end(after).ok_or_else(invalid_placeholder)?;
                    let token = &after[..end];
                    {
                        if !literal.is_empty() {
                            template
                                .parts
                                .push(Part::Literal(std::mem::take(&mut literal)));
                        }
                        let (kind, attrs) = attributes(token)?;
                        let part = match kind {
                            "clipboard" if attrs.is_empty() => Part::Clipboard,
                            "date" | "time" | "datetime"
                                if attrs.keys().all(|key| *key == "format") =>
                            {
                                Part::Date {
                                    kind: kind.into(),
                                    format: attrs.get("format").cloned(),
                                }
                            }
                            "timestamp" if attrs.is_empty() => Part::Date {
                                kind: "timestamp".into(),
                                format: None,
                            },
                            "argument" => {
                                let argument = Argument::from_attributes(&attrs)?;
                                let index = if let Some(index) = template
                                    .arguments
                                    .iter()
                                    .position(|item| item.name == argument.name)
                                {
                                    if template.arguments[index] != argument {
                                        return Err(tr!(
                                            "同名占位符的配置必须一致。",
                                            "Repeated arguments must use the same configuration."
                                        )
                                        .into());
                                    }
                                    index
                                } else {
                                    if template.arguments.len() >= 8 {
                                        return Err(tr!(
                                            "一个片段最多包含 8 个输入字段。",
                                            "A snippet can contain up to 8 input fields."
                                        )
                                        .into());
                                    }
                                    template.arguments.push(argument);
                                    template.arguments.len() - 1
                                };
                                Part::Argument(index)
                            }
                            _ => return Err(invalid_placeholder()),
                        };
                        template.parts.push(part);
                        rest = &after[end + 1..];
                        continue;
                    }
                }
            }
            let ch = rest.chars().next().unwrap();
            literal.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
        if !literal.is_empty() {
            template.parts.push(Part::Literal(literal));
        }
        Ok(template)
    }

    pub fn uses_clipboard(&self) -> bool {
        self.parts
            .iter()
            .any(|part| matches!(part, Part::Clipboard))
    }

    pub fn render(
        &self,
        clipboard: &str,
        values: &HashMap<String, String>,
        date: impl FnMut(&str, Option<&str>) -> String,
    ) -> Result<String, String> {
        self.render_inner(clipboard, values, date, false)
    }

    pub fn render_preview(
        &self,
        clipboard: &str,
        values: &HashMap<String, String>,
        date: impl FnMut(&str, Option<&str>) -> String,
    ) -> Result<String, String> {
        self.render_inner(clipboard, values, date, true)
    }

    fn render_inner(
        &self,
        clipboard: &str,
        values: &HashMap<String, String>,
        mut date: impl FnMut(&str, Option<&str>) -> String,
        preview: bool,
    ) -> Result<String, String> {
        let mut output = String::new();
        for part in &self.parts {
            let value = match part {
                Part::Literal(text) => std::borrow::Cow::Borrowed(text.as_str()),
                Part::Clipboard => std::borrow::Cow::Borrowed(clipboard),
                Part::Date { kind, format } => {
                    std::borrow::Cow::Owned(date(kind, format.as_deref()))
                }
                Part::Argument(index) => {
                    let arg = &self.arguments[*index];
                    let value = values
                        .get(&arg.name)
                        .or(arg.default.as_ref())
                        .map_or("", String::as_str);
                    if preview && value.trim().is_empty() && arg.required {
                        std::borrow::Cow::Owned(format!("⟨{}⟩", arg.name))
                    } else {
                        arg.validate_value(value, arg.required)?;
                        std::borrow::Cow::Borrowed(value)
                    }
                }
            };
            if output.len().saturating_add(value.len()) > MAX_RENDERED_BYTES {
                return Err(
                    tr!("展开后的片段超过 1 MB。", "Expanded snippet exceeds 1 MB.").into(),
                );
            }
            output.push_str(&value);
        }
        Ok(output)
    }
}

fn invalid_placeholder() -> String {
    tr!(
        "占位符格式无效。例如 {argument name=\"姓名\"} 或 {date format=\"yyyy-MM-dd\"}。",
        "Invalid placeholder. Try {argument name=\"Name\"} or {date format=\"yyyy-MM-dd\"}."
    )
    .into()
}

fn placeholder_end(text: &str) -> Option<usize> {
    let mut quoted = false;
    let mut escaped = false;
    for (index, ch) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted && ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            quoted = !quoted;
        } else if ch == '}' && !quoted {
            return Some(index);
        }
    }
    None
}

fn attributes(token: &str) -> Result<(&str, HashMap<&str, String>), String> {
    let end = token.find(char::is_whitespace).unwrap_or(token.len());
    let kind = &token[..end];
    let mut rest = token[end..].trim();
    let mut attrs = HashMap::new();
    while !rest.is_empty() {
        let (key, value) = rest.split_once('=').ok_or_else(invalid_placeholder)?;
        let key = key.trim();
        let value = value
            .trim_start()
            .strip_prefix('"')
            .ok_or_else(invalid_placeholder)?;
        let mut decoded = String::new();
        let mut chars = value.char_indices();
        let end = loop {
            let (index, ch) = chars.next().ok_or_else(invalid_placeholder)?;
            if ch == '"' {
                break index;
            }
            if ch == '\\' {
                let (_, next) = chars.next().ok_or_else(invalid_placeholder)?;
                match next {
                    'n' => decoded.push('\n'),
                    'r' => decoded.push('\r'),
                    't' => decoded.push('\t'),
                    '"' | '\\' => decoded.push(next),
                    _ => {
                        decoded.push('\\');
                        decoded.push(next);
                    }
                }
            } else {
                decoded.push(ch);
            }
        };
        let raw = format!("\"{}\"", &value[..end]);
        let decoded = serde_json::from_str::<String>(&raw).unwrap_or(decoded);
        if attrs.insert(key, decoded).is_some() {
            return Err(invalid_placeholder());
        }
        rest = value[end + 1..].trim_start();
    }
    Ok((kind, attrs))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snippet(name: &str, body: &str) -> Snippet {
        Snippet {
            id: name.into(),
            name: name.into(),
            body: body.into(),
        }
    }

    #[test]
    fn expands_dynamic_values_once_and_reuses_named_arguments() {
        let template = Template::parse("Hi {argument name=\"姓名\"}, {date format=\"yyyy-MM-dd\"}\n{clipboard}\n{argument name=\"姓名\"} {time} {datetime}").unwrap();
        assert_eq!(template.arguments.len(), 1);
        let values = HashMap::from([("姓名".into(), "你好 {date}".into())]);
        let rendered = template
            .render("{argument name=\"other\"}", &values, |kind, format| {
                format.unwrap_or(kind).into()
            })
            .unwrap();
        assert_eq!(
            rendered,
            "Hi 你好 {date}, yyyy-MM-dd\n{argument name=\"other\"}\n你好 {date} time datetime"
        );
    }
    #[test]
    fn requires_arguments_and_accepts_defaults() {
        let template =
            Template::parse("{argument name=\"who\"} {argument name=\"tone\" default=\"casual\"}")
                .unwrap();
        assert!(
            template
                .render("", &HashMap::new(), |_, _| unreachable!())
                .is_err()
        );
        assert_eq!(
            template
                .render(
                    "",
                    &HashMap::from([("who".into(), "Ada".into())]),
                    |_, _| unreachable!()
                )
                .unwrap(),
            "Ada casual"
        );
    }
    #[test]
    fn preserves_code_and_escaped_placeholders() {
        let template = Template::parse(
            "fn main() { println!(\"hi\"); }\n{\"a\": {\"b\": 1}} {unknown} \\{date} {clipboard}",
        )
        .unwrap();
        assert_eq!(
            template
                .render("", &HashMap::new(), |_, _| panic!())
                .unwrap(),
            "fn main() { println!(\"hi\"); }\n{\"a\": {\"b\": 1}} {unknown} {date} "
        );
    }
    #[test]
    fn rejects_invalid_templates_and_oversized_expansions() {
        for body in [
            "{argument name=bad}",
            "{date foo=\"bar\"}",
            "{argument",
            "{argument name=\"\"}",
            "{argument name=\"x\"}{argument name=\"x\" default=\"y\"}",
        ] {
            assert!(Template::parse(body).is_err(), "{body}");
        }
        let template = Template::parse("{clipboard}{clipboard}").unwrap();
        assert!(
            template
                .render(
                    &"a".repeat(MAX_RENDERED_BYTES),
                    &HashMap::new(),
                    |_, _| unreachable!()
                )
                .is_err()
        );
    }
    #[test]
    fn searches_names_before_body_without_polluting_empty_search() {
        let snippets = vec![
            snippet("other", "daily standup"),
            snippet("Daily", "hello"),
            snippet("Daily standup", "hello"),
        ];
        assert_eq!(matching(&snippets, "daily"), [1, 2, 0]);
        assert_eq!(matching(&snippets, "DAILY hello"), [1, 2]);
        assert!(matching(&snippets, " ").is_empty());
    }
    #[test]
    fn config_roundtrip_and_legacy_defaults() {
        let mut config = crate::config::Config {
            snippets: vec![snippet("Greeting", "Hello {argument name=\"Name\"}")],
            ..Default::default()
        };
        assert_eq!(
            crate::config::Config::from_json(&config.to_json().unwrap()).unwrap(),
            config
        );
        assert!(
            crate::config::Config::from_json("{}")
                .unwrap()
                .snippets
                .is_empty()
        );
        config.snippets.push(snippet("greeting", "duplicate"));
        assert!(config.to_json().is_err());
    }
    #[test]
    fn typed_fields_roundtrip_quotes_braces_unicode_and_defaults() {
        let fields = [
            Argument {
                name: "Name \"{姓名}\"".into(),
                default: Some("backslash \\ and tab\t".into()),
                required: false,
                kind: ArgumentKind::Text,
            },
            Argument {
                name: "Notes".into(),
                default: Some("line 1\nline 2\r\n{date} \u{0008}".into()),
                required: true,
                kind: ArgumentKind::Multiline,
            },
            Argument {
                name: "Tone".into(),
                default: Some("Say \"hi\"".into()),
                required: true,
                kind: ArgumentKind::Choice(vec![
                    "friendly".into(),
                    "Say \"hi\"".into(),
                    "a,b {date}".into(),
                ]),
            },
        ];
        for field in fields {
            let token = field.placeholder().unwrap();
            let template = Template::parse(&format!("{token}|{token}")).unwrap();
            assert_eq!(template.arguments.as_slice(), std::slice::from_ref(&field));
            assert_eq!(
                template
                    .render("", &HashMap::new(), |_, _| unreachable!())
                    .unwrap(),
                format!("{0}|{0}", field.default.unwrap())
            );
        }
    }

    #[test]
    fn legacy_defaults_and_explicit_optional_values() {
        let legacy =
            Template::parse("{argument name=\"Notes\" default=\"line 1\nline 2\"}").unwrap();
        assert_eq!(legacy.arguments[0].kind, ArgumentKind::Multiline);
        assert!(!legacy.arguments[0].required);
        assert_eq!(
            legacy
                .render("", &HashMap::new(), |_, _| unreachable!())
                .unwrap(),
            "line 1\nline 2"
        );
        let template =
            Template::parse(r#"{argument name="Extra" required="false" default="example"}"#)
                .unwrap();
        assert_eq!(
            template
                .render("", &HashMap::new(), |_, _| unreachable!())
                .unwrap(),
            "example"
        );
        assert_eq!(
            template
                .render(
                    "",
                    &HashMap::from([("Extra".into(), "".into())]),
                    |_, _| unreachable!()
                )
                .unwrap(),
            ""
        );
        let config = crate::config::Config {
            snippets: vec![snippet(
                "Legacy",
                "{argument name=\"Notes\" default=\"line 1\nline 2\"}",
            )],
            ..Default::default()
        };
        assert_eq!(
            crate::config::Config::from_json(&config.to_json().unwrap()).unwrap(),
            config
        );
    }

    #[test]
    fn validates_typed_values_and_previews_unfilled_required_choices() {
        let template = Template::parse(r#"{argument name="Tone" type="choice" options="friendly\nformal"} {argument name="Optional" required="false"}"#).unwrap();
        assert!(
            template
                .render("", &HashMap::new(), |_, _| unreachable!())
                .is_err()
        );
        assert_eq!(
            template
                .render_preview("", &HashMap::new(), |_, _| unreachable!())
                .unwrap(),
            "⟨Tone⟩ "
        );
        assert!(
            template
                .render(
                    "",
                    &HashMap::from([("Tone".into(), "bogus".into())]),
                    |_, _| unreachable!()
                )
                .is_err()
        );
        assert_eq!(
            template
                .render(
                    "",
                    &HashMap::from([("Tone".into(), "formal".into())]),
                    |_, _| unreachable!()
                )
                .unwrap(),
            "formal "
        );
        for token in [
            r#"{argument name="x" required="yes"}"#,
            r#"{argument name="x" type="unknown"}"#,
            r#"{argument name="x" type="text" options="a"}"#,
            r#"{argument name="x" type="text" default="a\nb"}"#,
            r#"{argument name="x" type="choice" options="a\na"}"#,
            r#"{argument name="x" type="choice" options="a\nb" default="c"}"#,
            r#"{argument name="x"}{argument name="x" required="false"}"#,
        ] {
            assert!(Template::parse(token).is_err(), "accepted {token}");
        }
    }

    #[test]
    fn date_presets_and_utf16_insertion_preserve_template_boundaries() {
        for preset in DATE_PRESETS {
            let template = Template::parse(preset.token).unwrap();
            assert!(template.arguments.is_empty());
            assert!(
                !template
                    .render("", &HashMap::new(), |kind, format| format
                        .unwrap_or(kind)
                        .into())
                    .unwrap()
                    .is_empty()
            );
        }
        let token = date_placeholder("yyyy-MM-dd 'at' HH:mm").unwrap();
        assert_eq!(
            inserting("😊text", 2, 4, &token).unwrap(),
            format!("😊{token}")
        );
        assert!(inserting("😊text", 1, 0, &token).is_err());
        assert!(inserting("", usize::MAX, 2, &token).is_err());
        assert!(date_placeholder("  ").is_err());
        let original = r#"{argument name="Name"}"#;
        let conflict = r#"{argument name="Name" default="Ada"}"#;
        assert!(inserting(original, 0, 0, conflict).is_err());
        assert!(inserting(&"a".repeat(MAX_BODY_BYTES), 0, 0, "{date}").is_err());
    }
}
