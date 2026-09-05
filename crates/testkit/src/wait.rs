//! Bounded waits on another process's progress — the one place a test
//! spells "until this is true, or fail saying what never happened".
//!
//! A test that drives a real `yunta` process cannot join it: the process
//! writes a marker, changes a status, or dies on its own schedule. Every
//! such wait polls a condition and yields between polls, never sleeps for
//! a guessed duration, and gives up after one shared deadline with the
//! caller's own words for what did not happen — composed at failure time,
//! so the message can carry the last thing the test observed.

use std::time::{Duration, Instant};

/// The longest any test waits for another process to make progress.
/// Generous next to the milliseconds a step takes, so a loaded runner
/// never trips it; short enough that a hang fails a test, not a job.
pub const WAIT_DEADLINE: Duration = Duration::from_secs(10);

/// Polls `observe` until it yields a value, and returns it. Panics after
/// [`WAIT_DEADLINE`] with the sentence `what` composes — "the bash node
/// never wrote its pid", or a message that quotes the last status seen.
pub fn wait_for<T>(mut observe: impl FnMut() -> Option<T>, what: impl FnOnce() -> String) -> T {
    let deadline = Instant::now() + WAIT_DEADLINE;
    loop {
        if let Some(value) = observe() {
            return value;
        }
        assert!(Instant::now() < deadline, "{}", what());
        std::thread::yield_now();
    }
}

/// Polls `condition` until it holds. Panics after [`WAIT_DEADLINE`] the
/// same way [`wait_for`] does.
pub fn wait_until(mut condition: impl FnMut() -> bool, what: impl FnOnce() -> String) {
    wait_for(|| condition().then_some(()), what);
}
