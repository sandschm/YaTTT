//! YaTTT core: everything that is not UI. No Tauri dependency, fully testable with `cargo test`.

pub mod error;
pub mod export;
pub mod i18n;
pub mod idle;
pub mod jira;
pub mod models;
pub mod rounding;
pub mod secrets;
pub mod settings;
pub mod stats;
pub mod storage;
pub mod sync;
pub mod tempo;
pub mod timer;

pub use error::{Error, Result};
pub use i18n::Lang;
pub use settings::TimeFormat;

/// Human readable duration in the current language: `1:05` (hours:minutes) or `1,08` / `1.08`
/// (decimal hours, German comma or English point).
pub fn format_duration(seconds: i64, format: TimeFormat) -> String {
    format_duration_in(seconds, format, i18n::lang())
}

pub fn format_duration_in(seconds: i64, format: TimeFormat, lang: Lang) -> String {
    let seconds = seconds.max(0);
    match format {
        TimeFormat::HoursMinutes => format!("{}:{:02}", seconds / 3600, (seconds % 3600) / 60),
        TimeFormat::Decimal => {
            let text = format!("{:.2}", seconds as f64 / 3600.0);
            match lang {
                Lang::De => text.replace('.', ","),
                Lang::En => text,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(format_duration(3900, TimeFormat::HoursMinutes), "1:05");
        assert_eq!(
            format_duration_in(3900, TimeFormat::Decimal, Lang::De),
            "1,08"
        );
        assert_eq!(
            format_duration_in(3900, TimeFormat::Decimal, Lang::En),
            "1.08"
        );
        assert_eq!(format_duration(-5, TimeFormat::HoursMinutes), "0:00");
    }
}
