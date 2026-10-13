//! The escalation options the engine builds. The reserved vocabulary is one
//! closed enum in the core crate, built from here and read back through it,
//! so no run-loop branch matches a human's choice or a node's outcome against
//! a bare string literal.

use yunta_core::events::{Decider, GateOption, HumanChoice};
use yunta_core::expansion::{node_offer, Decision, Denial};
use yunta_core::protected::Protected;
use yunta_core::reserved::ReservedOption;
use yunta_core::ScopeGlob;

/// The option `option` is offered as, with the words a person picks it by.
/// Private, so an offer below is the only way a worded option is built and
/// no caller can leave out what it trades off.
fn worded(
    option: ReservedOption,
    label: impl Into<String>,
    tradeoff: impl Into<String>,
) -> GateOption {
    GateOption {
        id: option.id(),
        label: Some(label.into()),
        tradeoff: Some(tradeoff.into()),
        asks: None,
    }
}

/// The options the engine offers, each with the words a person reads
/// before choosing it.
///
/// Every escalation the engine raises takes its options from here. That
/// is what makes "every option declares what it trades off" true by
/// construction rather than by remembering: a `GateOption` is built in
/// this module and nowhere else. The options of a failure's menu and of a
/// departure carry no words of their own: the surface that draws the menu
/// writes them from the data the menu holds.
pub(crate) mod offers {
    use yunta_core::events::GateOption;
    use yunta_core::reserved::ReservedOption;
    use yunta_core::NodeId;

    use super::worded;

    /// The option a failure's menu offers for `option`, with no words of
    /// its own: the surface that draws the menu writes them from the
    /// failure, and from the node the menu is about.
    pub(crate) fn failure_option(option: ReservedOption) -> GateOption {
        GateOption {
            id: option.id(),
            label: None,
            tradeoff: None,
            asks: None,
        }
    }

    /// Accept a departure from the plan. Its words come from the
    /// departure's menu.
    pub(crate) fn accept() -> GateOption {
        failure_option(ReservedOption::Accept)
    }

    /// Send a departure back to the session that made it. That session asks
    /// what should change: the person's words are what it picks its work
    /// back up with, and without them the lap would change nothing.
    pub(crate) fn send_back() -> GateOption {
        GateOption {
            asks: Some("what should it do instead?".to_string()),
            ..failure_option(ReservedOption::SendBack)
        }
    }

    /// Stop, on any escalation. It says `pauses` because that is what it
    /// does: the decision is recorded, the node is left stateless, and a
    /// resume asks again.
    pub(crate) fn abort() -> GateOption {
        worded(
            ReservedOption::Abort,
            "Abort the run",
            "Pauses here; nothing further executes",
        )
    }

    /// Spend past the run's token cap for this invocation.
    pub(crate) fn continue_past_tokens() -> GateOption {
        worded(
            ReservedOption::Continue,
            "Continue past the cap",
            "Lifts the cap for this invocation only; a later resume will ask again \
             before spending more",
        )
    }

    /// Stop spending, with the run parked on the cap.
    pub(crate) fn abort_on_tokens() -> GateOption {
        worded(
            ReservedOption::Abort,
            "Pause the run",
            "The run pauses with reason `budget`; a resume re-asks",
        )
    }

    /// Iterate past the loop's declared cap for this invocation.
    pub(crate) fn continue_past_iterations() -> GateOption {
        worded(
            ReservedOption::Continue,
            "Keep iterating",
            "Lifts the cap for this invocation only; a later resume will ask again",
        )
    }

    /// Stop iterating, failing the loop node on its own limit.
    pub(crate) fn abort_on_iterations() -> GateOption {
        worded(
            ReservedOption::Abort,
            "Fail the loop node",
            "The node fails naming the limit and the run pauses; a resume re-runs \
             the loop and re-asks",
        )
    }

    /// Pass a gate the forge could not be asked about.
    pub(crate) fn approve_from_console() -> GateOption {
        worded(
            ReservedOption::Approve,
            "Approve",
            "Marks the gate as passed; the run continues",
        )
    }

    /// Fail a gate the forge could not be asked about.
    pub(crate) fn reject_from_console() -> GateOption {
        worded(
            ReservedOption::Reject,
            "Reject",
            "Fails the node; its declared re-route (if any) takes over",
        )
    }

    /// Widen a task's scope to the paths it asked for.
    pub(crate) fn grant(paths: &str) -> GateOption {
        worded(
            ReservedOption::Grant,
            format!("Grant access to {paths}"),
            "The task's final diff is evaluated against its scope plus these paths",
        )
    }

    /// Refuse the widening, leaving the task to finish inside its scope.
    pub(crate) fn deny() -> GateOption {
        worded(
            ReservedOption::Deny,
            "Deny the request",
            "The denial becomes a finding; the task retries within its original scope",
        )
    }

    /// An author's own gate option, whose words come from the `on:`
    /// mapping the workflow declared for it: the node it sends the run
    /// back to, when it sends it back, and otherwise the nodes that wait
    /// on the gate — `next` — which is where the run goes once it is
    /// resolved. The label says where the option leads, so a menu never
    /// reads as an id followed by itself.
    ///
    /// One that sends the run back to a session asks what should change:
    /// the person's words are what that session picks its work back up
    /// with, and without them the lap would change nothing.
    pub(crate) fn declared(
        id: &yunta_core::OptionId,
        target: Option<&yunta_core::Node>,
        next: &[&NodeId],
    ) -> GateOption {
        let continues = target.filter(|target| target.kind.opens_resumable_session());
        let onward = yunta_core::text::listed(next.iter().map(|id| id.as_str()));
        let (label, tradeoff) = match (target, continues) {
            (Some(target), Some(_)) => (
                format!("Back to `{}`", target.id),
                format!(
                    "sends what you say back to `{}`, and asks again once it completes",
                    target.id
                ),
            ),
            (Some(target), None) => (
                format!("Back to `{}`", target.id),
                format!(
                    "re-routes to `{}` and asks again once it completes",
                    target.id
                ),
            ),
            (None, _) if next.is_empty() => (
                "Resolve the gate".to_string(),
                "resolves this gate; nothing in the workflow waits on it".to_string(),
            ),
            (None, _) => (
                format!("On to {onward}"),
                format!(
                    "resolves this gate; {onward} {} next",
                    yunta_core::text::agreeing(next.len(), "runs", "run")
                ),
            ),
        };
        GateOption {
            id: id.clone(),
            label: Some(label),
            tradeoff: Some(tradeoff),
            asks: continues.map(|_| "what should change?".to_string()),
        }
    }
}

/// What a person's choice decides about a request the loop escalated: the
/// grant the choice names, unless the protected set refuses one of its paths,
/// or else the refusal the choice is. A task's own request is always
/// widenable.
pub(crate) fn decision_of_choice(
    choice: &HumanChoice,
    paths: &[ScopeGlob],
    protected: &Protected,
    by: &Decider,
) -> Decision {
    if ReservedOption::of(&choice.option) != Some(ReservedOption::Grant) {
        return Decision::Deny(Denial::Chosen {
            option: choice.option.as_str().to_string(),
            note: choice.free_text.clone(),
        });
    }
    match node_offer(paths, protected, true) {
        Ok(granted) => Decision::Grant {
            paths: granted,
            by: by.clone(),
        },
        Err(denial) => Decision::Deny(denial),
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::events::GateOption;
    use yunta_core::reserved::ReservedOption;
    use yunta_core::{NodeId, OptionId};

    use super::*;

    #[test]
    fn a_task_grant_names_its_widening_and_no_cap() {
        // Only the engine's own grants count toward `max_per_run`, so a grant
        // a person picks from this menu does not say it consumes the cap.
        let grant = offers::grant("src/a.rs");
        assert_eq!(grant.id.as_str(), "grant");
        let tradeoff = grant.tradeoff.as_deref().unwrap_or_default();
        assert!(!tradeoff.contains("max_per_run"), "{tradeoff}");
    }

    /// Every option this module offers, built with stand-in arguments: the
    /// closed set an escalation draws from, so a new offer added without a
    /// place here is the one thing this test cannot see. A menu's own
    /// options are here too: the failure remedies and the departure answers.
    fn every_offer() -> Vec<GateOption> {
        let node: NodeId = "fix-lint".into();
        let declared_id = OptionId::from_static("ship-it");
        let target = |yaml: &str| -> yunta_core::Node { yunta_core::yaml::parse(yaml).unwrap() };
        let bash = target("{ id: fix-lint, kind: bash, run: \"true\" }");
        let prompt = target("{ id: plan, kind: prompt, prompt: \"plan it\" }");
        vec![
            offers::abort(),
            offers::continue_past_tokens(),
            offers::abort_on_tokens(),
            offers::continue_past_iterations(),
            offers::abort_on_iterations(),
            offers::approve_from_console(),
            offers::reject_from_console(),
            offers::grant("src/session/"),
            offers::deny(),
            offers::declared(&declared_id, Some(&bash), &[]),
            offers::declared(&declared_id, Some(&prompt), &[]),
            offers::declared(&declared_id, None, &[]),
            offers::declared(&declared_id, None, &[&node]),
            offers::failure_option(ReservedOption::Abort),
            offers::failure_option(ReservedOption::Retry),
            offers::failure_option(ReservedOption::ContinueWork),
            offers::failure_option(ReservedOption::Grant),
            offers::failure_option(ReservedOption::Promote),
            offers::accept(),
            offers::send_back(),
        ]
    }

    #[test]
    fn every_option_the_engine_offers_says_what_choosing_it_costs() {
        for option in every_offer() {
            match (option.label.as_deref(), option.tradeoff.as_deref()) {
                // A menu's own option: the surface that draws the menu writes its words.
                (None, None) => {}
                (Some(label), Some(tradeoff)) => {
                    assert!(
                        !tradeoff.trim().is_empty(),
                        "`{}` is offered without what it trades off, which is what makes \
                         the choice a decision rather than a guess",
                        option.id
                    );
                    assert!(
                        !label.trim().is_empty(),
                        "`{}` is offered with nothing to read on its row",
                        option.id
                    );
                }
                _ => panic!(
                    "`{}` is offered with a label or a tradeoff but not both",
                    option.id
                ),
            }
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
