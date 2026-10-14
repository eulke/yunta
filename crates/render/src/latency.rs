//! How long a rail call took, as a person reads it: milliseconds for the
//! calls that take less than a second, and the duration drawer's words for
//! the rest.

use std::time::Duration;

use yunta_core::units::duration;

/// A latency as a person reads it: whole milliseconds under a second, and
/// the duration helper's words from a second up, since that helper counts
/// in whole seconds and a rail call takes milliseconds.
pub fn latency(took: Duration) -> String {
    match (took.as_secs(), took.as_millis()) {
        (0, 0) => "under 1 ms".to_string(),
        (0, millis) => format!("{millis} ms"),
        _ => duration(took),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::latency;

    #[test]
    fn a_call_under_a_millisecond_reads_as_under_one() {
        assert_eq!(latency(Duration::from_micros(400)), "under 1 ms");
    }

    #[test]
    fn a_latency_reads_in_milliseconds_under_a_second() {
        assert_eq!(latency(Duration::from_micros(400)), "under 1 ms");
        assert_eq!(latency(Duration::from_millis(12)), "12 ms");
        assert_eq!(latency(Duration::from_millis(2_500)), "2s");
    }

    #[test]
    fn a_call_under_a_second_reads_in_whole_milliseconds() {
        assert_eq!(latency(Duration::from_millis(250)), "250 ms");
    }

    #[test]
    fn a_call_of_a_second_or_more_reads_in_the_duration_drawer_words() {
        assert_eq!(latency(Duration::from_secs(90)), "1m30s");
    }
}
