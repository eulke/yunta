//! The spans the host the run worked on was suspended, and the one rule
//! that leaves them out of a duration.

use std::time::Duration;

use chrono::{DateTime, Utc};

/// One suspension: it ended when the engine noticed the host awake again,
/// and lasted `slept` before that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suspension {
    pub woke_at: DateTime<Utc>,
    pub slept: Duration,
}

impl Suspension {
    /// When the host went to sleep.
    fn fell_asleep(&self) -> DateTime<Utc> {
        self.woke_at - self.slept
    }

    /// How much of `[from, to]` the host spent in this suspension.
    fn inside(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Duration {
        let start = self.fell_asleep().max(from);
        let end = self.woke_at.min(to);
        (end - start).to_std().unwrap_or(Duration::ZERO)
    }
}

/// Every suspension the run's log records, in the order it recorded them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Suspensions(Vec<Suspension>);

impl Suspensions {
    pub(crate) fn push(&mut self, suspension: Suspension) {
        self.0.push(suspension);
    }

    /// Every suspension, oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &Suspension> {
        self.0.iter()
    }

    /// How much of `[from, to]` the host spent suspended.
    pub fn asleep_between(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Duration {
        self.0
            .iter()
            .map(|suspension| suspension.inside(from, to))
            .sum()
    }

    /// The time from `from` to `to` the host was awake: the wall-clock
    /// interval less every suspension inside it. [`Duration::ZERO`] when
    /// `to` is the earlier of the two — nothing ran for a negative time.
    ///
    /// Every duration the run reports reads its intervals through this,
    /// so a machine that slept through a node never counts as the node
    /// working.
    pub fn awake_between(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Duration {
        let wall = (to - from).to_std().unwrap_or(Duration::ZERO);
        wall.saturating_sub(self.asleep_between(from, to))
    }

    /// How many suspensions the log records and how long they lasted in
    /// all; `None` when the host never slept.
    pub fn summary(&self) -> Option<(usize, Duration)> {
        (!self.0.is_empty()).then(|| {
            (
                self.0.len(),
                self.0.iter().map(|suspension| suspension.slept).sum(),
            )
        })
    }
}
