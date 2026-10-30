//! Rounding of durations for booking. Local data always stays exact.

use crate::settings::{Rounding, RoundingMode};

/// Round `seconds` according to `rounding`. A positive duration never rounds to zero
/// when rounding up or to nearest with a non-zero interval; this mirrors Tyme's behaviour
/// where a started task is worth at least one interval.
pub fn round_seconds(seconds: i64, rounding: Rounding) -> i64 {
    if rounding.minutes == 0 || seconds <= 0 {
        return seconds.max(0);
    }
    let step = rounding.minutes as i64 * 60;
    let rounded = match rounding.mode {
        RoundingMode::Nearest => ((seconds + step / 2) / step) * step,
        RoundingMode::Up => ((seconds + step - 1) / step) * step,
        RoundingMode::Down => (seconds / step) * step,
    };
    match rounding.mode {
        RoundingMode::Down => rounded,
        _ => rounded.max(step),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(min: u32, mode: RoundingMode) -> Rounding {
        Rounding { minutes: min, mode }
    }

    #[test]
    fn no_rounding() {
        assert_eq!(round_seconds(1234, r(0, RoundingMode::Nearest)), 1234);
        assert_eq!(round_seconds(-5, r(0, RoundingMode::Nearest)), 0);
    }

    #[test]
    fn nearest() {
        assert_eq!(round_seconds(7 * 60, r(15, RoundingMode::Nearest)), 15 * 60);
        assert_eq!(round_seconds(8 * 60, r(15, RoundingMode::Nearest)), 15 * 60);
        assert_eq!(
            round_seconds(22 * 60, r(15, RoundingMode::Nearest)),
            15 * 60
        );
        assert_eq!(
            round_seconds(23 * 60, r(15, RoundingMode::Nearest)),
            30 * 60
        );
        // tiny durations still count one interval
        assert_eq!(round_seconds(30, r(15, RoundingMode::Nearest)), 15 * 60);
    }

    #[test]
    fn up_and_down() {
        assert_eq!(round_seconds(61, r(5, RoundingMode::Up)), 300);
        assert_eq!(round_seconds(300, r(5, RoundingMode::Up)), 300);
        assert_eq!(round_seconds(301, r(5, RoundingMode::Up)), 600);
        assert_eq!(round_seconds(299, r(5, RoundingMode::Down)), 0);
        assert_eq!(round_seconds(899, r(5, RoundingMode::Down)), 600);
    }
}
