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
            return Vec::new();
        }
        tokens.push(token);
    }
    if tokens.is_empty() {
        return (0..windows.len()).collect();
    }
    let mut matches: Vec<(usize, i32)> = windows
        .iter()
        .enumerate()
        .filter_map(|(index, window)| {
            let app = Field::new(&window.app);
            let title = Field::new(&window.title);
            let mut score = 0;
            for token in &tokens {
                score += app.score(token).max(title.score(token))?;
            }
            if preferred == Some(window.id) {
                score += 300;
            }
            Some((index, score))
        })
        .collect();
    matches.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    matches.into_iter().map(|(index, _)| index).collect()
}

struct Field {
    chars: Vec<char>,
    word_starts: Vec<bool>,
    initials: Vec<char>,
}

impl Field {
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
}
