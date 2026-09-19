#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowInfo {
    pub id: u64,
    pub pid: i32,
    pub app: String,
    pub title: String,
    pub minimized: bool,
}

const MAX_FIELD_CHARS: usize = 512;
const MAX_QUERY_CHARS: usize = 128;

pub fn rank(windows: &[WindowInfo], query: &str, preferred: Option<u64>) -> Vec<usize> {
    let Some(query) = Query::new(query) else {
        return Vec::new();
    };
    if query.is_empty() {
        return (0..windows.len()).collect();
    }
    let mut matches: Vec<(usize, i32)> = windows
        .iter()
        .enumerate()
        .filter_map(|(index, window)| {
            let mut score = query.score([window.app.as_str(), window.title.as_str()])?;
            if preferred == Some(window.id) {
                score += 300;
            }
            Some((index, score))
        })
        .collect();
    matches.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    matches.into_iter().map(|(index, _)| index).collect()
}

pub(crate) struct Query {
    tokens: Vec<Vec<char>>,
}

impl Query {
    pub(crate) fn new(query: &str) -> Option<Self> {
        let mut tokens = Vec::new();
        let mut query_chars = 0;
        for token in query.split_whitespace() {
            let token: Vec<char> = token
                .chars()
                .flat_map(char::to_lowercase)
                .take(MAX_QUERY_CHARS + 1)
                .collect();
            query_chars += token.len();
            if query_chars > MAX_QUERY_CHARS {
                return None;
            }
            tokens.push(token);
        }
        Some(Self { tokens })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    pub(crate) fn window_match(&self, window: &WindowInfo) -> Option<(u8, i32)> {
        if self.matches_exactly(&window.app) {
            return Some((0, 0));
        }
        let app = Field::new(&window.app);
        if self
            .tokens
            .iter()
            .all(|token| app.contains(token) || app.initials.starts_with(token))
        {
            return Some((1, 0));
        }
        let title = Field::new(&window.title);
        if self
            .tokens
            .iter()
            .all(|token| app.contains(token) || title.contains(token))
        {
            return Some((2, 0));
        }
        let score = self.tokens.iter().try_fold(0, |score, token| {
            let app_score = app.score(token).map(|score| score + 250);
            Some(score + app_score.max(title.score(token))?)
        })?;
        Some((3, score))
    }

    fn matches_exactly(&self, text: &str) -> bool {
        let mut words = text.split_whitespace();
        self.tokens.iter().all(|token| {
            words.next().is_some_and(|word| {
                word.chars()
                    .flat_map(char::to_lowercase)
                    .eq(token.iter().copied())
            })
        }) && words.next().is_none()
    }

    pub(crate) fn score<'a>(&self, fields: impl IntoIterator<Item = &'a str>) -> Option<i32> {
        let fields: Vec<_> = fields.into_iter().map(Field::new).collect();
        self.tokens.iter().try_fold(0, |score, token| {
            Some(score + fields.iter().filter_map(|field| field.score(token)).max()?)
        })
    }
}

struct Field {
    chars: Vec<char>,
    word_starts: Vec<bool>,
    initials: Vec<char>,
}

impl Field {
    fn contains(&self, token: &[char]) -> bool {
        self.chars.windows(token.len()).any(|part| part == token)
    }

    fn new(text: &str) -> Self {
        let mut field = Self {
            chars: Vec::new(),
            word_starts: Vec::new(),
            initials: Vec::new(),
        };
        let mut previous = None::<char>;
        for ch in text.chars().take(MAX_FIELD_CHARS) {
            let word_start = ch.is_alphanumeric()
                && previous.is_none_or(|prev| {
                    !prev.is_alphanumeric() || (prev.is_lowercase() && ch.is_uppercase())
                });
            for (offset, lower) in ch.to_lowercase().enumerate() {
                field.chars.push(lower);
                field.word_starts.push(word_start && offset == 0);
                if word_start {
                    field.initials.push(lower);
                }
            }
            previous = Some(ch);
        }
        field
    }

    fn score(&self, token: &[char]) -> Option<i32> {
        if self.chars == token {
            return Some(12_000);
        }
        if self.chars.starts_with(token) {
            return Some(10_000);
        }
        let mut best = self
            .chars
            .windows(token.len())
            .enumerate()
            .filter(|(_, part)| *part == token)
            .map(|(index, _)| 8_000 + i32::from(self.word_starts[index]) * 1_000 - index as i32)
            .max();
        if self.initials.starts_with(token) {
            best = best.max(Some(9_500));
        }
        if best.is_some() {
            return best;
        }
        best = self.typo_score(token);
        let mut next = 0;
        let mut first = 0;
        for (index, ch) in self.chars.iter().enumerate() {
            if *ch == token[next] {
                if next == 0 {
                    first = index;
                }
                next += 1;
                if next == token.len() {
                    let gaps = index + 1 - first - token.len();
                    return best.max(Some(2_000 - gaps as i32 * 2 - first as i32));
                }
            }
        }
        best
    }

    fn typo_score(&self, token: &[char]) -> Option<i32> {
        let max_edits = match token.len() {
            0..=3 => return None,
            4..=7 => 1,
            _ => 2,
        };
        if !token.iter().all(|ch| ch.is_alphanumeric()) {
            return None;
        }
        let mut query = None;
        self.word_starts
            .iter()
            .enumerate()
            .filter(|(_, start)| **start)
            .filter_map(|(start, _)| {
                let end = (start + 1..self.chars.len())
                    .find(|&index| self.word_starts[index] || !self.chars[index].is_alphanumeric())
                    .unwrap_or(self.chars.len());
                let word = &self.chars[start..end];
                if word.len().abs_diff(token.len()) > max_edits {
                    return None;
                }
                // Compare words, not entire titles, so a long file title does not
                // penalize the project name at its end. Lengths are bounded above.
                let query = query.get_or_insert_with(|| token.iter().collect::<String>());
                let word_text: String = word.iter().collect();
                let distance = strsim::damerau_levenshtein(query, &word_text);
                if distance > max_edits {
                    return None;
                }
                let similarity = 1_000 - 1_000 * distance / token.len().max(word.len());
                Some(6_000 - 1_000 * distance as i32 + similarity as i32 / 2)
            })
            .max()
    }
}
