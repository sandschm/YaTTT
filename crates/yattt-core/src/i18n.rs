//! Language of the texts the core produces for users: error messages, CSV headers, durations.
//!
//! The language is a process-wide setting. The application resolves the user's choice (or the
//! system locale) once at start-up and again whenever the setting changes, then calls
//! [`set_lang`]. Threading a language parameter through every function would add noise without
//! benefit; functions that need to be deterministic in tests take an explicit [`Lang`].

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Lang {
    De = 0,
    #[default]
    En = 1,
}

static LANG: AtomicU8 = AtomicU8::new(Lang::En as u8);

/// Sets the language for all subsequently produced texts.
pub fn set_lang(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

/// The current language (English until [`set_lang`] is called).
pub fn lang() -> Lang {
    match LANG.load(Ordering::Relaxed) {
        0 => Lang::De,
        _ => Lang::En,
    }
}

impl Lang {
    /// Picks the language from a locale tag such as `de-DE`, `de_AT` or `en-US`.
    pub fn from_tag(tag: &str) -> Lang {
        if tag.trim().to_ascii_lowercase().starts_with("de") {
            Lang::De
        } else {
            Lang::En
        }
    }

    pub fn tag(self) -> &'static str {
        match self {
            Lang::De => "de",
            Lang::En => "en",
        }
    }
}

/// Picks the German or English text for the current language.
///
/// `tr!("Deutsch", "English")` yields a `&'static str`; with format arguments
/// (`tr!("Eintrag {id}", "Entry {id}", id = id)`) it yields a `String`.
#[macro_export]
macro_rules! tr {
    ($de:literal, $en:literal) => {
        match $crate::i18n::lang() {
            $crate::i18n::Lang::De => $de,
            $crate::i18n::Lang::En => $en,
        }
    };
    ($de:literal, $en:literal, $($arg:tt)+) => {
        match $crate::i18n::lang() {
            $crate::i18n::Lang::De => format!($de, $($arg)+),
            $crate::i18n::Lang::En => format!($en, $($arg)+),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_resolve_to_languages() {
        assert_eq!(Lang::from_tag("de-DE"), Lang::De);
        assert_eq!(Lang::from_tag("de_AT.UTF-8"), Lang::De);
        assert_eq!(Lang::from_tag("en-US"), Lang::En);
        assert_eq!(Lang::from_tag("fr-FR"), Lang::En);
        assert_eq!(Lang::from_tag(""), Lang::En);
    }
}
