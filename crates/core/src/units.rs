//! The figures a log keeps for a measured quantity: what an event records,
//! as opposed to how a person reads it, which is Surco's
//! (`surco::figures`).

use std::time::Duration;

/// `duration` in whole milliseconds, the figure a log keeps for a bound.
/// A duration past `u64::MAX` milliseconds reads as `u64::MAX`, so the
/// figure saturates rather than wrapping into a small number.
pub fn whole_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bound_is_counted_in_whole_milliseconds_and_saturates() {
        assert_eq!(whole_millis(Duration::from_micros(1_999)), 1);
        assert_eq!(whole_millis(Duration::from_millis(1_500)), 1_500);
        assert_eq!(whole_millis(Duration::MAX), u64::MAX);
    }
}
