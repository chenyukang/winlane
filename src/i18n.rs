use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    System,
    Chinese,
    English,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Locale {
    English,
    Chinese,
}

impl Language {
    pub fn resolve<'a>(self, preferred: impl IntoIterator<Item = &'a str>) -> Locale {
        match self {
            Self::Chinese => Locale::Chinese,
            Self::English => Locale::English,
            Self::System => preferred
                .into_iter()
                .find_map(
                    |tag| match tag.split(['-', '_']).next()?.to_ascii_lowercase().as_str() {
                        "zh" => Some(Locale::Chinese),
                        "en" => Some(Locale::English),
                        _ => None,
                    },
                )
                .unwrap_or(Locale::English),
        }
    }
}

static LOCALE: AtomicU8 = AtomicU8::new(0);

pub fn locale() -> Locale {
    if LOCALE.load(Ordering::Relaxed) == 1 {
        Locale::Chinese
    } else {
        Locale::English
    }
}

pub fn set_locale(locale: Locale) -> bool {
    let value = u8::from(locale == Locale::Chinese);
    LOCALE.swap(value, Ordering::Relaxed) != value
}

#[macro_export]
macro_rules! tr {
    ($chinese:literal, $english:literal) => {
        match $crate::i18n::locale() {
            $crate::i18n::Locale::Chinese => $chinese,
            $crate::i18n::Locale::English => $english,
        }
    };
}

#[macro_export]
macro_rules! trf {
    ($chinese:literal, $english:literal $(, $($args:tt)*)?) => {
        match $crate::i18n::locale() {
            $crate::i18n::Locale::Chinese => format!($chinese $(, $($args)*)?),
            $crate::i18n::Locale::English => format!($english $(, $($args)*)?),
        }
    };
}
