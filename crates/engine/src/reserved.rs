//! The escalation options the engine itself builds and interprets. Their
//! spellings live here once, so no run-loop branch ever matches a human's
//! choice or a node's outcome against a bare string literal — the reserved
//! vocabulary is one closed enum, built from it and read back through it.

use std::str::FromStr;

use yunta_core::events::GateOption;
use yunta_core::OptionId;

/// An option the engine appends to an escalation and acts on when a human
/// picks it — as opposed to an author-declared option, which the engine
/// only records. Everything the run loop decides on goes through this
/// enum, never a literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReservedOption {
    /// Pause the run with the decision recorded (every escalation offers it).
    Abort,
    /// Re-route to the corrective node once more, past the reroute cap.
    Retry,
    /// Accept promotion to the next declared mode.
    Promote,
    /// Approve a published gate.
    Approve,
    /// Grant a scope expansion a task requested.
    Grant,
    /// Deny a scope expansion a task requested.
    Deny,
    /// Lift a budget cap for this invocation only.
    Continue,
    /// Reject a published gate resolved from the console.
    Reject,
}

impl ReservedOption {
    /// The YAML/log spelling — the one place each option's text is
    /// written, for both building an escalation and comparing a choice.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ReservedOption::Abort => "abort",
            ReservedOption::Retry => "retry",
            ReservedOption::Promote => "promote",
            ReservedOption::Approve => "approve",
            ReservedOption::Grant => "grant",
            ReservedOption::Deny => "deny",
            ReservedOption::Continue => "continue",
            ReservedOption::Reject => "reject",
        }
    }

    /// The option as an escalation offers it.
    pub(crate) fn id(self) -> OptionId {
        OptionId::from_static(self.as_str())
    }

    /// The reserved option `id` spells, or `None` for an author's own.
    pub(crate) fn of(id: &OptionId) -> Option<Self> {
        id.as_str().parse().ok()
    }

    /// The option this one is offered as, with the words a person picks
    /// it by. Private, so an offer below is the only way one is built
    /// and no caller can leave out what it trades off.
    fn offer(self, label: impl Into<String>, tradeoff: impl Into<String>) -> GateOption {
        GateOption {
            id: self.id(),
            label: label.into(),
            tradeoff: tradeoff.into(),
        }
    }
}

/// The options the engine offers, each with the words a person reads
/// before choosing it.
///
/// Every escalation the engine raises takes its options from here. That
/// is what makes "every option declares what it trades off" true by
/// construction rather than by remembering: a `GateOption` is built in
/// this module and nowhere else, and the one builder each goes through
/// has no way to spell an empty one.
pub(crate) mod offers {
    use yunta_core::events::GateOption;
    use yunta_core::{ModeName, NodeId};

    use super::ReservedOption;

    /// Stop, on any escalation. It says `pauses` because that is what it
    /// does: the decision is recorded, the node is left stateless, and a
    /// resume asks again.
    pub(crate) fn abort() -> GateOption {
        ReservedOption::Abort.offer("Abort the run", "Pauses here; nothing further executes")
    }

    /// Send a failed node back through its corrective node once more,
    /// past the cap the workflow declared.
    pub(crate) fn retry(goto: &NodeId, max_reroutes: u32) -> GateOption {
        ReservedOption::Retry.offer(
            format!("Re-route to `{goto}` once more"),
            format!(
                "Uses one extra correction attempt beyond the declared max_reroutes \
                 ({max_reroutes}); escalates again if `{goto}` doesn't fix it"
            ),
        )
    }

    /// Close this run and start its successor in the next declared mode.
    pub(crate) fn promote(next_mode: &ModeName, mode_name: &ModeName) -> GateOption {
        ReservedOption::Promote.offer(
            format!("Promote to mode `{next_mode}`"),
            format!(
                "Closes this run (`run_finished: promoted`) and starts a successor in \
                 `{next_mode}`, inheriting this run's artifacts — there's no \
                 mechanism to demote back to `{mode_name}`"
            ),
        )
    }

    /// Spend past the run's token cap for this invocation.
    pub(crate) fn continue_past_tokens() -> GateOption {
        ReservedOption::Continue.offer(
            "Continue past the cap",
            "Lifts the cap for this invocation only; a later resume will ask again \
             before spending more",
        )
    }

    /// Stop spending, with the run parked on the cap.
    pub(crate) fn abort_on_tokens() -> GateOption {
        ReservedOption::Abort.offer(
            "Pause the run",
            "The run pauses with reason `budget`; a resume re-asks",
        )
    }

    /// Iterate past the loop's declared cap for this invocation.
    pub(crate) fn continue_past_iterations() -> GateOption {
        ReservedOption::Continue.offer(
            "Keep iterating",
            "Lifts the cap for this invocation only; a later resume will ask again",
        )
    }

    /// Stop iterating, failing the loop node on its own limit.
    pub(crate) fn abort_on_iterations() -> GateOption {
        ReservedOption::Abort.offer(
            "Fail the loop node",
            "The node fails naming the limit and the run pauses; a resume re-runs \
             the loop and re-asks",
        )
    }

    /// Pass a gate the forge could not be asked about.
    pub(crate) fn approve_from_console() -> GateOption {
        ReservedOption::Approve.offer("Approve", "Marks the gate as passed; the run continues")
    }

    /// Fail a gate the forge could not be asked about.
    pub(crate) fn reject_from_console() -> GateOption {
        ReservedOption::Reject.offer(
            "Reject",
            "Fails the node; its declared re-route (if any) takes over",
        )
    }

    /// Widen a task's scope to the paths it asked for.
    pub(crate) fn grant(paths: &str) -> GateOption {
        ReservedOption::Grant.offer(
            format!("Grant access to {paths}"),
            "The task's final diff is evaluated against its scope plus these paths",
        )
    }

    /// Refuse the widening, leaving the task to finish inside its scope.
    pub(crate) fn deny() -> GateOption {
        ReservedOption::Deny.offer(
            "Deny the request",
            "The denial becomes a finding; the task retries within its declared scope",
        )
    }

    /// An author's own gate option, whose words come from the `on:`
    /// mapping the workflow declared for it.
    pub(crate) fn declared(id: &yunta_core::OptionId, target: Option<&NodeId>) -> GateOption {
        GateOption {
            id: id.clone(),
            label: id.to_string(),
            tradeoff: match target {
                Some(target) => {
                    format!("re-routes to `{target}` and asks again once it completes")
                }
                None => "resolves this gate; the flow continues".to_string(),
            },
        }
    }
}

impl FromStr for ReservedOption {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "abort" => Ok(ReservedOption::Abort),
            "retry" => Ok(ReservedOption::Retry),
            "promote" => Ok(ReservedOption::Promote),
            "approve" => Ok(ReservedOption::Approve),
            "grant" => Ok(ReservedOption::Grant),
            "deny" => Ok(ReservedOption::Deny),
            "continue" => Ok(ReservedOption::Continue),
            "reject" => Ok(ReservedOption::Reject),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::events::GateOption;
    use yunta_core::{ModeName, NodeId, OptionId};

    use super::*;

    /// Every option this module offers, built with stand-in arguments —
    /// the closed set an escalation draws from, so a new offer added
    /// without a place here is the one thing this test cannot see.
    fn every_offer() -> Vec<GateOption> {
        let node: NodeId = "fix-lint".into();
        let mode: ModeName = "standard".into();
        let next: ModeName = "ship".into();
        let declared_id = OptionId::from_static("ship-it");
        vec![
            offers::abort(),
            offers::retry(&node, 0),
            offers::promote(&next, &mode),
            offers::continue_past_tokens(),
            offers::abort_on_tokens(),
            offers::continue_past_iterations(),
            offers::abort_on_iterations(),
            offers::approve_from_console(),
            offers::reject_from_console(),
            offers::grant("src/session/"),
            offers::deny(),
            offers::declared(&declared_id, Some(&node)),
            offers::declared(&declared_id, None),
        ]
    }

    #[test]
    fn every_option_the_engine_offers_says_what_choosing_it_costs() {
        for option in every_offer() {
            assert!(
                !option.tradeoff.trim().is_empty(),
                "`{}` is offered without what it trades off, which is what makes \
                 the choice a decision rather than a guess",
                option.id
            );
            assert!(
                !option.label.trim().is_empty(),
                "`{}` is offered with nothing to read on its row",
                option.id
            );
        }
    }

    #[test]
    fn an_offer_is_answered_by_the_id_the_engine_reads_back() {
        for option in every_offer() {
            let id = &option.id;
            assert!(
                ReservedOption::of(id).is_some() || id.as_str() == "ship-it",
                "`{id}` is offered but no reserved option spells it, so the run \
                 loop cannot act on the answer"
            );
        }
    }
}
