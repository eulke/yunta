//! Deterministic clocks, so no assertion ever races the wall clock.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use yunta_core::Clock;

/// The instant [`FixedClock`] always reports.
pub const FIXED_NOW: &str = "2026-01-01T00:00:00Z";

/// [`FIXED_NOW`] as an instant — the fixed origin a test measures from,
/// whether it takes it through a [`Clock`] or stamps a log with it
/// directly.
pub fn fixed_now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(FIXED_NOW)
        .expect("FIXED_NOW is a valid RFC3339 timestamp")
        .with_timezone(&Utc)
}

/// A [`Clock`] frozen at [`FIXED_NOW`] — the default for a test that only
/// needs time to be constant, not any particular value.
#[derive(Default)]
pub struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        fixed_now()
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

/// A [`Clock`] for a host a test puts to sleep: its wall and awake
/// readings stand still until the test moves them, together — the host
/// working — or the wall alone — the host suspended.
pub struct HostClock {
    wall: Mutex<DateTime<Utc>>,
    awake: Mutex<Instant>,
}

impl Default for HostClock {
    /// Starts at [`FIXED_NOW`], awake since the moment it is built.
    fn default() -> Self {
        HostClock {
            wall: Mutex::new(fixed_now()),
            awake: Mutex::new(Instant::now()),
        }
    }
}

impl HostClock {
    /// The host works for `by`: both readings move.
    pub fn advance(&self, by: Duration) {
        *self.wall.lock().expect("the wall reading") += by;
        *self.awake.lock().expect("the awake reading") += by;
    }

    /// The host sleeps for `for_`: the wall moves and the awake reading
    /// does not.
    pub fn suspend(&self, for_: Duration) {
        *self.wall.lock().expect("the wall reading") += for_;
    }
}

impl Clock for HostClock {
    fn now(&self) -> DateTime<Utc> {
        *self.wall.lock().expect("the wall reading")
    }

    fn awake(&self) -> Instant {
        *self.awake.lock().expect("the awake reading")
    }
}
