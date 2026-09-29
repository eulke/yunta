//! Whether the host a run works on slept: two readings of its clock — wall
//! time and the time the host was awake — taken between events and as the
//! run waits, compared.
//!
//! The process's monotonic clock stands still while the host is suspended,
//! so a wall clock that outran it between two readings says the host slept
//! for the difference. Nothing here asks the operating system anything.
//!
//! A host that just woke is not trusted to stay awake: a laptop wakes for
//! a minute of maintenance and sleeps again, and a session opened in that
//! minute hangs until its timeout. No session opens until the host has
//! been awake [`SETTLE_AFTER_SUSPENSION`] since it last slept.

use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use tokio_util::sync::CancellationToken;
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

/// How long a host that slept has to stay awake before a session opens on
/// it: longer than the wakes a laptop makes for maintenance, short next
/// to the sleep it follows.
pub const SETTLE_AFTER_SUSPENSION: Duration = Duration::from_secs(120);

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
    /// When, on the awake clock, the host last woke: the reading that
    /// noticed its latest suspension. `None` while it has not slept.
    woke: Mutex<Option<Instant>>,
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
            woke: Mutex::new(None),
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
        let slept = slept_between(&earlier, &now)?;
        *self.woke.lock().unwrap_or_else(|e| e.into_inner()) = Some(now.awake);
        Some(slept)
    }

    /// Whether the host has stayed awake [`SETTLE_AFTER_SUSPENSION`] since
    /// it last woke — at once for a host this run has not seen sleep.
    fn settled_now(&self) -> bool {
        let woke = *self.woke.lock().unwrap_or_else(|e| e.into_inner());
        woke.is_none_or(|woke| {
            self.clock.awake().saturating_duration_since(woke) >= SETTLE_AFTER_SUSPENSION
        })
    }

    /// Waits, reading the clock every [`LOOK`], until the host has
    /// settled — `false` when `cancel` fires first. The first reading is
    /// taken at once, so a suspension that ended just before is noticed
    /// here and waited out.
    pub(crate) async fn settled(
        &self,
        log: &RunLog<'_>,
        cancel: &CancellationToken,
    ) -> Result<bool, StorageError> {
        let mut look = tokio::time::interval(LOOK);
        look.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                biased;
                () = cancel.cancelled() => return Ok(false),
                _ = look.tick() => {}
            }
            log.note_suspension().await?;
            if self.settled_now() {
                return Ok(true);
            }
        }
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

    fn slept_an_hour(host: &Arc<yunta_testkit_core::HostClock>) -> Wakefulness {
        let awake = Wakefulness::new(host.clone());
        host.suspend(Duration::from_secs(3_600));
        assert!(awake.observe(host.now()).is_some(), "the hour is noticed");
        awake
    }

    #[test]
    fn a_host_never_seen_asleep_is_settled() {
        let host = Arc::new(yunta_testkit_core::HostClock::default());
        assert!(Wakefulness::new(host).settled_now());
    }

    #[test]
    fn a_host_settles_once_awake_long_enough_after_it_woke() {
        let host = Arc::new(yunta_testkit_core::HostClock::default());
        let awake = slept_an_hour(&host);

        host.advance(SETTLE_AFTER_SUSPENSION - Duration::from_secs(1));
        assert!(!awake.settled_now(), "a second short");
        host.advance(Duration::from_secs(1));
        assert!(awake.settled_now());
    }

    #[test]
    fn sleeping_again_starts_the_count_over() {
        let host = Arc::new(yunta_testkit_core::HostClock::default());
        let awake = slept_an_hour(&host);
        host.advance(SETTLE_AFTER_SUSPENSION - Duration::from_secs(1));

        host.suspend(Duration::from_secs(600));
        assert!(awake.observe(host.now()).is_some());
        host.advance(Duration::from_secs(1));

        assert!(!awake.settled_now(), "awake one second since it last woke");
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
