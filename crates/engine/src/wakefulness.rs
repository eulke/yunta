//! Whether the host a run works on slept: two readings of its clock — wall
//! time and the time the host was awake — taken between events and as the
//! run waits, compared.
//!
//! The process's monotonic clock stands still while the host is suspended,
//! so a wall clock that outran it between two readings says the host slept
//! for the difference. Nothing here asks the operating system anything.

use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use yunta_core::Clock;
use yunta_storage::StorageError;

use crate::run_log::RunLog;

/// How far the wall clock has to outrun the time the host was awake,
/// between two readings, to count as the host having slept: well above the
/// drift between two clocks of one machine, well below a sleep worth
/// telling a person about.
const SUSPENDED_AFTER: Duration = Duration::from_secs(10);

/// How often a run reads its clock while nothing else happens.
const LOOK: Duration = Duration::from_secs(1);

/// One reading of both clocks.
#[derive(Debug, Clone, Copy)]
struct Sample {
    wall: DateTime<Utc>,
    awake: Instant,
}

/// How long the host slept between two readings, when it did: the wall
/// time that passed less the awake time that did. A wall clock set back
/// says nothing about sleep.
fn slept_between(earlier: &Sample, later: &Sample) -> Option<Duration> {
    let wall = (later.wall - earlier.wall).to_std().ok()?;
    let awake = later.awake.saturating_duration_since(earlier.awake);
    let slept = wall.checked_sub(awake)?;
    (slept >= SUSPENDED_AFTER).then_some(slept)
}

/// The last reading of a run's clock, shared by everything that appends
/// to its log, so a suspension is noticed once, by whichever reading comes
/// first after the host wakes.
pub(crate) struct Wakefulness {
    clock: Arc<dyn Clock>,
    last: Mutex<Sample>,
}

impl Wakefulness {
    pub(crate) fn new(clock: Arc<dyn Clock>) -> Self {
        let first = Sample {
            wall: clock.now(),
            awake: clock.awake(),
        };
        Wakefulness {
            clock,
            last: Mutex::new(first),
        }
    }

    /// Takes a reading at `wall`: how long the host slept since the last
    /// one, if it did.
    pub(crate) fn observe(&self, wall: DateTime<Utc>) -> Option<Duration> {
        let now = Sample {
            wall,
            awake: self.clock.awake(),
        };
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        let earlier = std::mem::replace(&mut *last, now);
        slept_between(&earlier, &now)
    }

    /// Reads the clock every [`LOOK`] for as long as the run is driven,
    /// recording a suspension on `log` as soon as one is noticed — a run
    /// that appends nothing for an hour still says when its host slept.
    pub(crate) async fn watch(&self, log: &RunLog<'_>) -> Result<Infallible, StorageError> {
        let mut look = tokio::time::interval(LOOK);
        look.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            look.tick().await;
            log.note_suspension().await?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(wall_secs: i64, awake: Instant) -> Sample {
        Sample {
            wall: DateTime::from_timestamp(wall_secs, 0).expect("a valid instant"),
            awake,
        }
    }

    #[test]
    fn an_hour_the_wall_clock_outran_the_awake_clock_is_an_hour_asleep() {
        let awake = Instant::now();
        assert_eq!(
            slept_between(
                &reading(0, awake),
                &reading(3_601, awake + Duration::from_secs(1))
            ),
            Some(Duration::from_secs(3_600))
        );
    }

    #[test]
    fn a_host_awake_throughout_slept_no_time() {
        let awake = Instant::now();
        assert_eq!(
            slept_between(
                &reading(0, awake),
                &reading(60, awake + Duration::from_secs(60))
            ),
            None
        );
    }

    #[test]
    fn a_gap_under_the_threshold_is_no_suspension() {
        let awake = Instant::now();
        assert_eq!(slept_between(&reading(0, awake), &reading(9, awake)), None);
    }

    #[test]
    fn a_wall_clock_set_back_is_no_suspension() {
        let awake = Instant::now();
        assert_eq!(
            slept_between(
                &reading(3_600, awake),
                &reading(0, awake + Duration::from_secs(1))
            ),
            None
        );
    }
}
