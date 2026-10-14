//! The words of a session and of an account's limit: a session that died,
//! refused, ran past its limit or its timeout, or spent past its token
//! budget, and an attempt that repeats one that failed on an unchanged tree.

use std::time::Duration;

use yunta_core::events::{
    AccountLimit, Failure, LimitSource, RefusalCause, SessionDeath, SessionEnd, SessionRefusal,
    Timed, Unchanged,
};
use yunta_core::text;
use yunta_core::units::duration;

use crate::pause::RESET_FORMAT;

/// A session that ended without a terminal event: whose it was, and how
/// its process went when it had one of its own.
pub fn session_death(death: &SessionDeath) -> String {
    let Some(exit) = &death.exit else {
        return format!("session `{}` ended without a terminal event", death.adapter);
    };
    let said = format!(
        "session `{}` {} before any terminal event",
        death.adapter,
        session_end(&exit.end)
    );
    match exit.stderr_tail.last() {
        Some(last) => format!("{said} — {last}"),
        None => said,
    }
}

/// How a session's process ended: the code it exited with, or the signal
/// that killed it.
pub fn session_end(end: &SessionEnd) -> String {
    match end {
        SessionEnd::Code { code } => format!("exited with code {code}"),
        SessionEnd::Signal { signal } => format!("was killed by signal {signal}"),
        SessionEnd::Unknown => "ended in a way this build does not know".to_string(),
    }
}

/// A session's refusal: the adapter that refused, what the refusal was
/// for, and the message it gave.
pub(crate) fn refusal_sentence(refusal: &SessionRefusal) -> String {
    let said = match &refusal.cause {
        RefusalCause::Final => format!("session `{}` refused the request", refusal.adapter),
        RefusalCause::Authentication => format!(
            "session `{}` refused the request (its credential was refused)",
            refusal.adapter
        ),
        RefusalCause::InvalidModel => format!(
            "session `{}` refused the request (the model is not one the account can use)",
            refusal.adapter
        ),
        RefusalCause::Unreachable => format!(
            "session `{}` refused the request (the service could not be reached)",
            refusal.adapter
        ),
    };
    text::detailed(said, &refusal.message)
}

/// An account's limit: who hit it, until when when the source named a
/// moment, and the message the source gave.
pub(crate) fn limit_sentence(limit: &AccountLimit) -> String {
    let who = match &limit.source {
        LimitSource::Session { adapter } => format!("session `{adapter}` hit a limit"),
        LimitSource::Forge => "the forge rate-limited its requests".to_string(),
    };
    let said = match &limit.resets_at {
        Some(at) => format!("{who} until {}", at.format(RESET_FORMAT)),
        None => who,
    };
    text::detailed(said, &limit.message)
}

/// The unchanged attempt's sentence: why it did not run, ending in what
/// the attempt that last ran failed with, as `failed` says it.
pub(crate) fn unchanged_sentence(since: u32, failed: &str) -> String {
    format!(
        "not run again: nothing in the run's tree changed since attempt {since} failed on \
         it — change what it failed on there first. Attempt {since} failed: {failed}"
    )
}

/// The unchanged attempt, with the sentence of the failure it repeats.
pub(super) fn unchanged_said(unchanged: &Unchanged) -> String {
    unchanged_sentence(unchanged.since, &super::sentence(&unchanged.failure))
}

/// A session or an executor that ran past its wall-clock bound, in the words
/// of the bound it ran past: a session's timeout, or an executor's, in whole
/// seconds.
pub(super) fn timed_out(after_ms: u64, timed: &Timed) -> String {
    match timed {
        Timed::Session => format!(
            "exceeded timeout of {}",
            duration(Duration::from_millis(after_ms))
        ),
        Timed::Executor { executor } => format!(
            "executor `{executor}` exceeded its {}s timeout",
            after_ms / 1000
        ),
    }
}

/// A task that spent past its token budget: what it used, and the cap.
pub(super) fn token_budget(used: u64, max: u64) -> String {
    format!("exceeded max_tokens {max} ({used} used)")
}

/// The engine could not stop a session it asked to stop.
pub(super) fn not_stopped(failure: &Failure) -> String {
    let Failure::SessionNotStopped {
        session_not_stopped,
    } = failure
    else {
        unreachable!("routed here only for a session the engine could not stop")
    };
    format!("the engine could not stop the session: {session_not_stopped}")
}
