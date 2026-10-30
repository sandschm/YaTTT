//! YaTTT core: everything that is not UI. No Tauri dependency, fully testable with `cargo test`.

pub mod error;
pub mod export;
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
pub use settings::TimeFormat;

/// Human readable duration: `1:05` (hours:minutes) or `1,08` (decimal hours).
pub fn format_duration(seconds: i64, format: TimeFormat) -> String {
    let seconds = seconds.max(0);
    match format {
        TimeFormat::HoursMinutes => format!("{}:{:02}", seconds / 3600, (seconds % 3600) / 60),
        TimeFormat::Decimal => format!("{:.2}", seconds as f64 / 3600.0).replace('.', ","),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(format_duration(3900, TimeFormat::HoursMinutes), "1:05");
        assert_eq!(format_duration(3900, TimeFormat::Decimal), "1,08");
        assert_eq!(format_duration(-5, TimeFormat::HoursMinutes), "0:00");
    }
}
