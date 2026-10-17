//! The windows a rail call keeps, and the budget a write's answer draws on.
//!
//! A CLI's hook timeout counts the hook process's own startup as well as
//! the call, so the write window is measured from the process's first
//! instant. The margins are set so that the engine's answer, a failure
//! report and the startup all land inside the CLI's timeout.

use std::time::{Duration, Instant};

/// The CLI's own timeout for a write hook, past the window and its margins.
pub const WRITE_HOOK_CLI_MS: u64 = 10_000;
/// The engine's answer to a write must land before the CLI's hook timeout.
pub const WRITE_ASK_MS: u64 = 7_000;
/// The failure report gets its own budget, so it cannot push the hook past its timeout.
pub const REPORT_MS: u64 = 1_500;
/// Covers the hook process's own startup, which the CLI's timeout also counts.
pub const START_MARGIN_MS: u64 = 1_000;
/// One write's formatters share this bound, so the engine answers before the window ends.
pub const FORMAT_MS: u64 = 5_000;
/// The stop-judgement bound when the config leaves the window unset: one hour.
pub const UNSET_WINDOW_SECS: u64 = 3_600;

// The answer, the failure report and the hook's startup fit inside the CLI's timeout.
const _: () = assert!(WRITE_ASK_MS + REPORT_MS + START_MARGIN_MS < WRITE_HOOK_CLI_MS);
// The formatters finish before the answer is due, so the engine can still give it.
const _: () = assert!(FORMAT_MS < WRITE_ASK_MS);

/// The CLI's hook timeout for a write hook, in whole seconds.
pub const fn write_hook_cli_seconds() -> u32 {
    (WRITE_HOOK_CLI_MS / 1000) as u32
}

/// What a CLI reads as a hook's timeout: the window in whole seconds, rounded up,
/// and never under one, because a zero timeout gives no hook time to answer.
pub fn hook_seconds(span: Duration) -> u64 {
    span.as_secs()
        .saturating_add(u64::from(span.subsec_nanos() > 0))
        .max(1)
}

/// The time a write's answer may take, counted from the hook process's start.
#[derive(Debug, Clone, Copy)]
pub struct AskBudget {
    until: Instant,
}

impl AskBudget {
    /// The budget of a hook process that started at `started`.
    pub fn from_process_start(started: Instant) -> Self {
        Self {
            until: started + Duration::from_millis(WRITE_ASK_MS),
        }
    }

    /// What is left at `now`, or `None` once the window is spent.
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        self.until
            .checked_duration_since(now)
            .filter(|left| !left.is_zero())
    }
}

/// How a call to the engine is bounded.
#[derive(Debug, Clone, Copy)]
pub enum Wait {
    /// A write's answer, within what is left of its window.
    Write(AskBudget),
    /// A stop's judgement. The CLI's connection has no deadline of its own:
    /// the stop hook process is killed at the stop window, its call timeout.
    Judgement,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hook_timeout_rounds_up_and_is_never_zero() {
        assert_eq!(hook_seconds(Duration::ZERO), 1);
        assert_eq!(hook_seconds(Duration::from_millis(300)), 1);
        assert_eq!(hook_seconds(Duration::from_millis(1_500)), 2);
        assert_eq!(hook_seconds(Duration::from_secs(3)), 3);
    }

    #[test]
    fn a_write_budget_is_spent_when_the_window_ends_from_the_process_start() {
        let started = Instant::now();
        let budget = AskBudget::from_process_start(started);
        let window = Duration::from_millis(WRITE_ASK_MS);
        assert_eq!(budget.remaining(started), Some(window));
        assert_eq!(budget.remaining(started + window), None);
    }
}
