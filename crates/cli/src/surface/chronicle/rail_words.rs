//! The words a session's rail is said in: a call of the rail that failed,
//! and what the rail heard of the session.

use std::collections::BTreeMap;

use yunta_core::rail::hook::RailFailureCause;
use yunta_core::rail::timing::Histogram;
use yunta_core::rail::RailEvent;
use yunta_core::text::counted;
use yunta_core::ModelName;

/// A failed call of the rail: the moment it was for, when the log read
/// one, and the cause the log closed it under.
pub(super) fn rail_failed(event: Option<RailEvent>, cause: RailFailureCause) -> String {
    let cause = cause_words(cause);
    match event {
        Some(event) => format!("rail failed {}: {cause}", moment_words(event)),
        None => format!("rail failed: {cause}"),
    }
}

/// What the rail heard of a session: how many calls it answered, and the
/// model it heard when a hook named one.
pub(super) fn rail_summary(
    model: Option<&ModelName>,
    moments: &BTreeMap<RailEvent, Histogram>,
) -> String {
    let calls: u64 = moments.values().map(|moment| moment.count).sum();
    let answered = counted(usize::try_from(calls).unwrap_or(usize::MAX), "call");
    match model {
        Some(model) => format!("the rail answered {answered}, model {model}"),
        None => format!("the rail answered {answered}"),
    }
}

/// The moment a call was for, in the words a person reads.
fn moment_words(event: RailEvent) -> &'static str {
    match event {
        RailEvent::BeforeWrite => "before a write",
        RailEvent::AfterWrite => "after a write",
        RailEvent::BeforeStop => "before a stop",
    }
}

/// The cause a failure closed under, in the words a person reads.
fn cause_words(cause: RailFailureCause) -> &'static str {
    match cause {
        RailFailureCause::Decode => "a call the engine could not read",
        RailFailureCause::Unauthorized => "a call without the session's token",
        RailFailureCause::Timeout => "no answer within its window",
        RailFailureCause::FormatterFailed => "the project's formatter did not start",
        RailFailureCause::ListenerDied => "the engine's listener stopped",
        RailFailureCause::JudgementFailed => "the stop could not be judged",
        RailFailureCause::NoFence => "no fence to judge the write",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use yunta_core::rail::hook::RailFailureCause;
    use yunta_core::rail::RailEvent;

    use super::{rail_failed, rail_summary};

    /// Every moment a rail call can be for, and every cause a failure closes
    /// under, reads in its own words, so none of them reads as a code.
    #[test]
    fn every_rail_moment_and_cause_reads_in_its_own_words() {
        for (event, words) in [
            (RailEvent::BeforeWrite, "before a write"),
            (RailEvent::AfterWrite, "after a write"),
            (RailEvent::BeforeStop, "before a stop"),
        ] {
            assert_eq!(
                rail_failed(Some(event), RailFailureCause::Timeout),
                format!("rail failed {words}: no answer within its window")
            );
        }
        for (cause, words) in [
            (RailFailureCause::Decode, "a call the engine could not read"),
            (
                RailFailureCause::Unauthorized,
                "a call without the session's token",
            ),
            (RailFailureCause::Timeout, "no answer within its window"),
            (
                RailFailureCause::FormatterFailed,
                "the project's formatter did not start",
            ),
            (
                RailFailureCause::ListenerDied,
                "the engine's listener stopped",
            ),
            (
                RailFailureCause::JudgementFailed,
                "the stop could not be judged",
            ),
            (RailFailureCause::NoFence, "no fence to judge the write"),
        ] {
            assert_eq!(rail_failed(None, cause), format!("rail failed: {words}"));
        }
    }

    #[test]
    fn a_summary_with_no_model_says_its_calls_alone() {
        assert_eq!(
            rail_summary(None, &BTreeMap::new()),
            "the rail answered 0 calls"
        );
    }
}
