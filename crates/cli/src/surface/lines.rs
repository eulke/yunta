//! The append-only surface: one line per event, nothing redrawn.
//!
//! This is what a pipe, a CI log, `TERM=dumb` and a screen reader get,
//! and it is the same content the pinned region carries — laid out for a
//! reader who arrives after the fact instead of one watching. Every line
//! opens with the run's elapsed time at that event, so a log read back
//! hours later still says where the time went.
//!
//! Its first line names the downgrade. A surface that quietly did less
//! than it can would be the silent degradation the contract's invariant
//! I11 forbids, so the reason travels with the change.

use std::io::Write;

use chrono::{DateTime, Utc};
use yunta_core::events::{EventPayload, GateResolvedPayload, StoredEvent};
use yunta_core::text::{detailed, one_line};

use crate::render::format_duration;

use super::{view, write_line};

/// One line per event, written as the event arrives.
pub(super) struct Lines {
    out: Box<dyn Write + Send>,
    /// The first event's instant — what every line's elapsed counts from.
    /// `None` until the first line, because a run with no event has no
    /// clock to start.
    opened: Option<DateTime<Utc>>,
}

impl Lines {
    /// Opens the surface on `out`, announcing what the reader is getting
    /// instead of the live region and why.
    pub(super) fn open(mut out: Box<dyn Write + Send>, reason: &str) -> Self {
        write_line(
            &mut out,
            &format!("live view off ({reason}): one line per event"),
        );
        Self { out, opened: None }
    }

    /// Writes one diagnostic the run raised, as its own line.
    ///
    /// No elapsed time in front of it: the column an event's line opens
    /// with is that event's position in the run, and a diagnostic is
    /// not an event on the log.
    pub(super) fn note(&mut self, line: &str) {
        write_line(&mut self.out, line);
    }

    /// Writes `event`'s line.
    pub(super) fn event(&mut self, event: &StoredEvent) {
        let opened = *self.opened.get_or_insert(event.timestamp);
        let elapsed = (event.timestamp - opened).to_std().unwrap_or_default();
        let mut line = format!("[{}] {}", format_duration(elapsed), event.body.kind_name());
        if let Some(node) = &event.node_id {
            line.push_str(&format!(" on `{node}`"));
        }
        if let Some(detail) = event.payload().and_then(detail) {
            line.push_str(&format!(" — {}", one_line(&detail)));
        }
        write_line(&mut self.out, &line);
    }
}

/// The free text a payload carries, or `None` when it says nothing.
///
/// A kind whose whole detail is one field off the log has nothing to add
/// once that field is blank, and the line reads as one that carries no
/// detail at all: the dash a line joins its detail with promises a
/// reader exactly what a colon does.
fn said(text: &str) -> Option<String> {
    Some(one_line(text)).filter(|text| !text.is_empty())
}

/// What one event carries beyond its kind and its node: the fact a reader
/// scanning the log acts on.
///
/// `None` where the kind says everything it has to say — the event still
/// gets its line, because a kind this binary cannot detail is still an
/// event that happened, and one it does not know at all still has a name.
///
/// A payload whose free text is empty — a log a writer left a blank
/// `cause` or `reason` on — is named by what the line already knows, and
/// never by a separator with nothing behind it: a kind whose whole
/// detail is that field answers `None`, and one that joins the field to
/// a headline drops the colon.
fn detail(payload: &EventPayload) -> Option<String> {
    match payload {
        EventPayload::RunCreated(p) => Some(format!("mode `{}` off {}", p.mode, p.base_branch)),
        EventPayload::RunnerResolved(p) => Some(format!(
            "{} on {}/{}",
            p.runner, p.chosen.adapter, p.chosen.model
        )),
        EventPayload::NodeStarted(p) => Some(format!("attempt {}", p.attempt)),
        EventPayload::AgentMessage(p) => p
            .tool_name
            .as_ref()
            .map(|tool| format!("{:?} {tool}", p.message_type)),
        EventPayload::ArtifactWritten(p) => Some(p.path.display().to_string()),
        EventPayload::TaskRegistered(p) => Some(p.task_id.to_string()),
        EventPayload::TaskStatusChanged(p) => Some(format!("{} is {:?}", p.task_id, p.new_status)),
        EventPayload::CriteriaChecked(p) => Some(format!(
            "{} {:?}: {} criteria",
            p.task_id,
            p.phase,
            p.results.len()
        )),
        EventPayload::ScopeChecked(p) => {
            Some(format!("{} path(s) out of scope", p.violations.len()))
        }
        EventPayload::NodeFinished(p) => said(&p.outcome),
        EventPayload::NodeFailed(p) => said(&p.failure.to_string()),
        EventPayload::HookExecuted(p) => Some(format!("{:?} exit {}", p.phase, p.exit_code)),
        EventPayload::NodeRerouted(p) => Some(detailed(format!("to `{}`", p.to_node), &p.cause)),
        EventPayload::GateWaiting(p) => said(&p.summary),
        EventPayload::GateResolved(p) => Some(resolution(p)),
        EventPayload::LoopIteration(p) => Some(format!("iteration {}", p.iteration)),
        EventPayload::FindingPosted(p) => Some(detailed(
            format!("{:?}", p.finding.severity),
            &p.finding.title,
        )),
        EventPayload::PromotionSignaled(p) => {
            Some(detailed(format!("to `{}`", p.suggested_mode), &p.reason))
        }
        EventPayload::ChildRunCreated(p) => Some(p.child_run_id.to_string()),
        EventPayload::ChildRunFinished(p) => Some(format!(
            "{} {}",
            p.child_run_id,
            view::closed_as(p.terminal_state)
        )),
        EventPayload::CapabilityDegraded(p) => Some(detailed(
            format!("{:?} on {}", p.capability, p.adapter),
            &p.policy_applied,
        )),
        EventPayload::RunPaused(p) => said(&p.reason),
        EventPayload::RunFinished(p) => Some(view::closed_as(p.terminal_state).to_string()),
        EventPayload::BaselineCaptured(_)
        | EventPayload::AgentSessionOpened(_)
        | EventPayload::ContextAssembled(_)
        | EventPayload::ScopeExpansionRequested(_)
        | EventPayload::ScopeExpansionGranted(_)
        | EventPayload::ScopeExpansionDenied(_)
        | EventPayload::QuestionsAnswered(_)
        | EventPayload::RunResumed(_) => None,
    }
}

/// How a gate was settled: who settled it and what they said, in each of
/// the shapes the persisted object spells.
fn resolution(payload: &GateResolvedPayload) -> String {
    match payload {
        GateResolvedPayload::Chosen(choice) => {
            format!("`{}` chosen by {}", choice.option, choice.by)
        }
        GateResolvedPayload::Approved { by, sha } => format!("approved by {by} over {sha}"),
        GateResolvedPayload::ChangesRequested { by } => format!("changes requested by {by}"),
        GateResolvedPayload::Closed => "closed without merging".to_string(),
        GateResolvedPayload::Unrecognized(_) => {
            "settled in a shape this binary does not name".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::events::{
        CapabilityDegradedPayload, EventPayload, Finding, FindingPostedPayload, FindingSeverity,
        NodeFinishedPayload, NodeReroutedPayload, PromotionSignaledPayload, RerouteOrigin,
        RunPausedPayload, TokenUsage,
    };
    use yunta_core::Capability;

    use super::detail;

    /// A log this binary reads back was written by some other
    /// invocation: nothing guarantees the free text on a payload says
    /// anything, and a line built for it must not promise that it does.
    fn rerouted(cause: &str) -> EventPayload {
        EventPayload::NodeRerouted(NodeReroutedPayload {
            to_node: "fix-lint".into(),
            cause: cause.to_string(),
            attempt: None,
            max_reroutes: None,
            origin: RerouteOrigin::GateChoice,
        })
    }

    #[test]
    fn a_reroute_with_a_cause_reads_as_the_target_and_the_cause() {
        assert_eq!(
            detail(&rerouted("exit 1")).as_deref(),
            Some("to `fix-lint`: exit 1")
        );
    }

    #[test]
    fn a_reroute_with_no_cause_recorded_reads_as_the_target_alone() {
        assert_eq!(detail(&rerouted("")).as_deref(), Some("to `fix-lint`"));
    }

    #[test]
    fn a_promotion_with_no_reason_recorded_reads_as_the_mode_alone() {
        let payload = EventPayload::PromotionSignaled(PromotionSignaledPayload {
            reason: String::new(),
            evidence: String::new(),
            suggested_mode: "ship".into(),
        });
        assert_eq!(detail(&payload).as_deref(), Some("to `ship`"));
    }

    #[test]
    fn a_finding_with_no_title_reads_as_its_severity_alone() {
        let payload = EventPayload::FindingPosted(FindingPostedPayload {
            finding: Finding {
                id: "f1".into(),
                severity: FindingSeverity::Minor,
                title: String::new(),
                location: "src/lib.rs".to_string(),
                detail: String::new(),
                proposed_criterion: None,
            },
        });
        assert_eq!(detail(&payload).as_deref(), Some("Minor"));
    }

    #[test]
    fn a_pause_with_no_reason_recorded_leaves_the_line_at_its_kind() {
        let payload = EventPayload::RunPaused(RunPausedPayload {
            reason: "   ".to_string(),
        });
        assert_eq!(detail(&payload), None);
    }

    #[test]
    fn a_pause_that_recorded_a_reason_says_it_on_one_line() {
        let payload = EventPayload::RunPaused(RunPausedPayload {
            reason: "budget\n  reached".to_string(),
        });
        assert_eq!(detail(&payload).as_deref(), Some("budget reached"));
    }

    #[test]
    fn a_node_that_finished_saying_nothing_leaves_the_line_at_its_kind() {
        let payload = EventPayload::NodeFinished(NodeFinishedPayload {
            outcome: String::new(),
            tokens_used: TokenUsage::default(),
        });
        assert_eq!(detail(&payload), None);
    }

    #[test]
    fn a_degraded_capability_with_no_policy_recorded_reads_as_what_was_missing() {
        let payload = EventPayload::CapabilityDegraded(CapabilityDegradedPayload {
            capability: Capability::RunTools,
            adapter: "codex".into(),
            policy_applied: String::new(),
        });
        assert_eq!(detail(&payload).as_deref(), Some("RunTools on codex"));
    }
}
