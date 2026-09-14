//! Deterministic clocks, so no assertion ever races the wall clock.

use chrono::{DateTime, Utc};
use yunta_core::Clock;

/// The instant [`FixedClock`] always reports.
pub const FIXED_NOW: &str = "2026-01-01T00:00:00Z";

/// A [`Clock`] frozen at [`FIXED_NOW`] — the default for a test that only
/// needs time to be constant, not any particular value.
pub struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(FIXED_NOW)
            .expect("FIXED_NOW is a valid RFC3339 timestamp")
            .with_timezone(&Utc)
    }
}

/// A [`Clock`] frozen at a caller-chosen instant — for a test that asserts
/// on the exact timestamp an event carries, or that needs two runs stamped
/// at different times.
pub struct AtClock(pub DateTime<Utc>);

impl AtClock {
    /// Builds an [`AtClock`] from an RFC3339 string, panicking if it does
    /// not parse — a test writes a literal it controls.
    pub fn rfc3339(instant: &str) -> Self {
        AtClock(
            DateTime::parse_from_rfc3339(instant)
                .expect("a valid RFC3339 timestamp")
                .with_timezone(&Utc),
        )
    }
}

impl Clock for AtClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}
