//! See [`crate`]. How a caller stops an operation, in the two stages a
//! Ctrl-C has.

use tokio_util::sync::CancellationToken;

/// How a caller stops an operation, in the two stages a Ctrl-C has: stop
/// what runs, then abort what tears it down. Two stages, because what gives
/// a checkout's lock back or hands a run over runs *because* the work
/// stopped, and one stage for both would cut the cleanup with the work.
///
/// A clone shares the stages: an interface trips the one it holds, and every
/// operation it handed a clone to sees it.
#[derive(Debug, Clone, Default)]
pub struct Stop {
    work: CancellationToken,
    teardown: CancellationToken,
}

impl Stop {
    /// Stages nothing has tripped.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stops what runs: every process an operation started to do its work.
    pub fn stop(&self) {
        self.work.cancel();
    }

    /// Aborts what stopping still holds, and stops what runs if nothing had.
    pub fn abort(&self) {
        self.work.cancel();
        self.teardown.cancel();
    }

    /// Whether what runs was stopped.
    pub fn is_stopped(&self) -> bool {
        self.work.is_cancelled()
    }

    /// Whether what tears the work down was aborted.
    pub fn is_aborted(&self) -> bool {
        self.teardown.is_cancelled()
    }

    /// What everything an operation starts to do its work answers to.
    pub fn work(&self) -> &CancellationToken {
        &self.work
    }

    /// What gives a take back once the work stopped answers to: it survives
    /// [`Stop::stop`] and falls to [`Stop::abort`].
    pub fn teardown(&self) -> &CancellationToken {
        &self.teardown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopping_leaves_the_teardown_standing_and_aborting_trips_it() {
        let stop = Stop::new();
        let held = stop.clone();
        assert!(!held.is_stopped() && !held.is_aborted());

        stop.stop();
        assert!(held.is_stopped(), "a clone sees the stage its source trips");
        assert!(
            !held.is_aborted(),
            "what tears the work down runs because it stopped"
        );

        stop.abort();
        assert!(held.is_aborted());
    }

    #[test]
    fn aborting_alone_stops_the_work_too() {
        let stop = Stop::new();
        stop.abort();
        assert!(stop.is_stopped() && stop.is_aborted());
    }
}
