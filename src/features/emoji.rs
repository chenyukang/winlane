use std::sync::OnceLock;

/// Keep native row creation bounded even for broad searches such as "face".
pub const MAX_RESULTS: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Emoji {
    pub text: &'static str,
    pub name_en: &'static str,
    pub name_zh: &'static str,
    keywords: &'static str,
    skin_tone: bool,
}

impl Emoji {
    pub fn name(&self) -> &'static str {
        match crate::core::i18n::locale() {
            crate::core::i18n::Locale::English => self.name_en,
            crate::core::i18n::Locale::Chinese => self.name_zh,
        }
    }
}

pub fn catalog() -> &'static [Emoji] {
    static CATALOG: OnceLock<Vec<Emoji>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        include_str!("../../resources/emoji/catalog.tsv")
            .lines()
            .map(|line| {
                let mut fields = line.split('\t');
                let text = fields.next().expect("emoji text");
                Emoji {
                    text,
                    name_en: fields.next().expect("English emoji name"),
                    name_zh: fields.next().expect("Chinese emoji name"),
                    keywords: fields.next().expect("emoji keywords"),
                    skin_tone: text
                        .chars()
                        .any(|c| ('\u{1f3fb}'..='\u{1f3ff}').contains(&c)),
                }
            })
            .collect()
    })
}

pub fn matching(query: &str) -> Vec<Emoji> {
    let raw = query.trim();
    let query = raw
        .to_lowercase()
        .replace(['_', ':', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let tokens: Vec<_> = query.split_whitespace().collect();
    let mut matches: Vec<_> = catalog()
        .iter()
        .enumerate()
        .filter_map(|(index, emoji)| {
            let score = if raw.is_empty() {
                // A few useful choices precede the standard Unicode order.
                const COMMON: &[&str] = &[
                    "😀", "😊", "😂", "🤣", "❤️", "👍", "🙏", "🎉", "🔥", "✅", "👀", "🚀", "🤔",
                    "😅", "🥰", "😭", "👋", "💪", "🙌", "💯",
                ];
                if emoji.skin_tone {
                    return None;
                }
                COMMON
                    .iter()
                    .position(|&text| text == emoji.text)
                    .unwrap_or(COMMON.len())
            } else if raw == emoji.text
                || raw
                    .chars()
                    .eq(emoji.text.chars().filter(|&c| c != '\u{fe0f}'))
            {
                0
            } else if tokens.is_empty()
                || !tokens.iter().all(|token| emoji.keywords.contains(token))
            {
                return None;
            } else if emoji
                .keywords
                .split(" | ")
                .take(2)
                .any(|name| name == query)
            {
                1
            } else if emoji.keywords.split(" | ").any(|keyword| keyword == query) {
                2
            } else if emoji.keywords.starts_with(&query) {
                3
            } else {
                4
            };
            Some(((score, emoji.skin_tone, index), *emoji))
        })
        .collect();
    matches.sort_unstable_by_key(|(rank, _)| *rank);
    matches
        .into_iter()
        .take(MAX_RESULTS)
        .map(|(_, emoji)| emoji)
        .collect()
}
