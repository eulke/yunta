//! The words for one moment, chosen once.
//!
//! Every surface that says what happened says it from here: the
//! append-only lines a pipe gets, and the history a watched terminal
//! keeps above its region. They differ in what they *keep* and in how
//! they lay it out, never in what a thing is called — so a reader who
//! followed a run on a terminal and a reader who read the same run out
//! of a CI log met the same sentences.
//!
//! Every state word comes from `render::state::NodeDisplay`, every
//! run or child close from `view::closed_as`, every capability from
//! `Capability::as_str`. Nothing here formats a domain type with
//! `Debug`: a reader is owed a word, not a Rust identifier.

mod words;

use yunta_core::events::{children, findings, gates, node, run, session};
use yunta_core::text::{aside, one_line};
use yunta_engine::{Happening, Moment};

use super::view;
use crate::render::{indent, Glyphs, StateWord};
use words::carried;

/// How deep a child sits under the node that bore it.
const CHILD_DEPTH: usize = 1;

/// One moment as a surface says it, before any layout decides where it
/// goes.
pub(super) struct Said {
    /// The state this moment is marked with, when it is about
    /// something reaching one; `None` for a moment that only reports.
    pub(super) word: Option<StateWord>,
    pub(super) text: String,
}

/// The words for `moment`. One match per domain, and the only ones.
pub(super) fn say(moment: &Moment) -> Said {
    let subject = match &moment.node {
        Some(node) => node.to_string(),
        None => "run".to_string(),
    };
    let (word, carried) = carried(&moment.happening);
    Said {
        word,
        text: aside(subject, &one_line(&carried)),
    }
}

/// Whether a terminal a person is watching keeps this above its region.
///
/// What it keeps is what closed something or asked something of a
/// person (D164): the rest is the run working, and the region already
/// shows that while it is true. A run's own close is the one thing that
/// closes and is not kept — the block that reports it is its record,
/// and a line above the region would say it twice.
pub(super) fn kept(happening: &Happening) -> bool {
    match happening {
        Happening::Run(run::happening::Happening::Paused { .. })
        | Happening::Run(run::happening::Happening::Resumed { .. })
        | Happening::Run(run::happening::Happening::PromotionSignaled { .. }) => true,
        Happening::Run(_) => false,
        Happening::Node(node::happening::Happening::Reached { state, .. }) => {
            !matches!(state, yunta_core::events::NodeState::Running { .. })
        }
        Happening::Node(node::happening::Happening::Rerouted(_)) => true,
        Happening::Node(_) => false,
        Happening::Session(session::happening::Happening::Degraded { .. }) => true,
        Happening::Session(_) => false,
        Happening::Gates(gates::happening::Happening::Escalated(_))
        | Happening::Gates(gates::happening::Happening::Resolved(_))
        | Happening::Gates(gates::happening::Happening::Asked { .. })
        | Happening::Gates(gates::happening::Happening::Answered { .. }) => true,
        Happening::Findings(findings::happening::Happening::Finding { change, .. }) => matches!(
            change,
            findings::happening::Change::Posted | findings::happening::Change::Withdrawn { .. }
        ),
        Happening::Children(children::happening::Happening::Closed { .. }) => true,
        Happening::Children(_) => false,
        Happening::Unknown { .. } => true,
        Happening::Tasks(_) | Happening::Scope(_) | Happening::Artifacts(_) => false,
    }
}

/// The rows a settled node leaves behind: its own line, and the
/// children it bore indented under it.
///
/// The children come with it because they leave the region with it. A
/// node in the region carries its own tree; a node that graduated
/// carries it into the history, where the run's composition stays
/// readable after the node that composed it is gone.
pub(super) fn graduation(moment: &Moment, glyphs: Glyphs) -> Vec<String> {
    let said = say(moment);
    let mark = said
        .word
        .map(|word| format!("{} ", glyphs.state(word)))
        .unwrap_or_default();
    let mut rows = vec![format!("{mark}{}", said.text)];
    let Happening::Node(node::happening::Happening::Reached { children, .. }) = &moment.happening
    else {
        return rows;
    };
    let under = indent(CHILD_DEPTH);
    rows.extend(
        children
            .iter()
            .map(|child| format!("{under}{}", view::child_row(child, glyphs))),
    );
    rows
}

#[cfg(test)]
mod tests {
    use yunta_core::events::{
        EventBody, EventPayload, Evidence, Failure, Finding, FindingPostedPayload, FindingSeverity,
        NodeEvent, NodeFinishedPayload, NodeReroutedPayload, PromotionSignaledPayload,
        RerouteCause, RerouteOrigin, RunEvent, StoredEvent,
    };
    use yunta_core::events::{FindingEvent, TokenUsage};
    use yunta_engine::chronicle as derive_chronicle;

    use super::*;

    /// What a surface says for one event, through the one derivation
    /// every surface reads: the log, folded, then the words.
    fn said_for(payload: EventPayload) -> String {
        let events = vec![StoredEvent {
            seq: 1.into(),
            run_id: "01JQ0000000000000000000000".into(),
            node_id: None,
            timestamp: chrono::DateTime::UNIX_EPOCH,
            body: EventBody::Known(payload),
        }];
        let moments = derive_chronicle(&events);
        say(moments.first().expect("one moment per event")).text
    }

    /// A log this binary reads back was written by some other
    /// invocation: nothing guarantees the free text on a payload says
    /// anything, and words built for it must not promise that it does.
    fn rerouted(cause: &str) -> EventPayload {
        EventPayload::Node(NodeEvent::Rerouted(NodeReroutedPayload::new(
            "fix-lint".into(),
            RerouteCause(Failure::message(cause)),
            RerouteOrigin::GateChoice,
            None,
            None,
        )))
    }

    #[test]
    fn a_reroute_with_a_cause_reads_as_the_target_and_the_cause() {
        assert_eq!(
            said_for(rerouted("exit 1")),
            "run — rerouted to `fix-lint`: exit 1"
        );
    }

    #[test]
    fn a_reroute_with_no_cause_recorded_reads_as_the_target_alone() {
        assert_eq!(said_for(rerouted("")), "run — rerouted to `fix-lint`");
    }

    #[test]
    fn a_promotion_with_no_reason_recorded_reads_as_the_mode_alone() {
        let payload = EventPayload::Run(RunEvent::PromotionSignaled(PromotionSignaledPayload {
            reason: String::new(),
            evidence: Evidence::none(),
            suggested_mode: "ship".into(),
        }));
        assert_eq!(said_for(payload), "run — promotion to `ship`");
    }

    #[test]
    fn a_finding_with_no_title_reads_as_its_severity_alone() {
        let payload = EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
            finding: Finding {
                id: "f1".into(),
                severity: FindingSeverity::Minor,
                title: String::new(),
                location: "src/lib.rs".into(),
                detail: String::new(),
                proposed_criterion: None,
            },
        }));
        assert_eq!(said_for(payload), "run — finding f1 minor");
    }

    #[test]
    fn a_node_that_finished_saying_nothing_is_still_said_to_have_finished() {
        let payload = EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
            String::new(),
            TokenUsage::default(),
        )));
        assert_eq!(said_for(payload), "run — finished");
    }

    /// A log written before the engine named its fallbacks carries an
    /// empty `policy_applied`. The words state what was missing and
    /// claim nothing about what was done instead.
    #[test]
    fn a_degraded_capability_with_no_policy_recorded_reads_as_what_was_missing() {
        let payload: EventPayload = serde_json::from_value(serde_json::json!({
            "kind": "capability_degraded",
            "capability": "run_tools",
            "adapter": "codex",
            "policy_applied": "",
        }))
        .expect("the wire form of a degradation with no policy");
        assert_eq!(said_for(payload), "run — run_tools not declared by codex");
    }

    #[test]
    fn every_kind_earns_its_words_once() {
        // One sentence per kind, and every one of them a sentence: a
        // kind nothing says words for would reach a reader as a bare
        // subject, and a `Debug` spelling would reach them as a Rust
        // identifier.
        for payload in yunta_testkit_core::all_kinds() {
            let kind = payload.kind_name();
            let said = said_for(payload);
            assert!(
                said.contains('—') || said != "run",
                "`{kind}` says nothing beyond its subject: {said:?}"
            );
            assert!(
                !said.contains('{') && !said.contains("::"),
                "`{kind}` reached a reader as a Rust value: {said:?}"
            );
        }
    }

    #[test]
    fn what_a_watched_terminal_keeps_is_what_closed_or_asked() {
        // P6, stated once: a terminal keeps above its region what
        // closed something or asked something of a person. Everything
        // else is the run working, and the region shows that while it
        // is true.
        let opened = EventPayload::Session(yunta_core::events::SessionEvent::Opened(
            serde_json::from_value(serde_json::json!({
                "kind": "agent_session_opened",
                "session_id": "s1",
                "capabilities": {},
            }))
            .expect("a session that opened"),
        ));
        let events = vec![StoredEvent {
            seq: 1.into(),
            run_id: "01JQ0000000000000000000000".into(),
            node_id: Some("work".into()),
            timestamp: chrono::DateTime::UNIX_EPOCH,
            body: EventBody::Known(opened),
        }];
        assert!(
            !kept(&derive_chronicle(&events)[0].happening),
            "a session opening is the run working"
        );

        let failed = EventPayload::Node(NodeEvent::Failed(
            yunta_core::events::NodeFailedPayload::new(
                Failure::message("exit 1"),
                false,
                TokenUsage::default(),
            ),
        ));
        let events = vec![StoredEvent {
            body: EventBody::Known(failed),
            ..events[0].clone()
        }];
        assert!(
            kept(&derive_chronicle(&events)[0].happening),
            "a node that failed closed something"
        );
    }
}
