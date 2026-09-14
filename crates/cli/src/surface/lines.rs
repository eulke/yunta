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
use yunta_core::fence::Coverage;

use chrono::{DateTime, Utc};
use yunta_core::events::{EventPayload, GateResolvedPayload, StoredEvent};
use yunta_core::text::{aside, detailed, one_line};

use crate::render::format_duration;

use super::{view, write_line};
use yunta_core::events::{
    ArtifactEvent, ChildEvent, FindingEvent, GateEvent, NodeEvent, RunEvent, ScopeEvent,
    SessionEvent, TaskEvent,
};

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
        let detail = event.payload().and_then(detail).unwrap_or_default();
        write_line(&mut self.out, &aside(line, &one_line(&detail)));
    }
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
        EventPayload::Run(RunEvent::Created(p)) => {
            Some(format!("mode `{}` off {}", p.mode, p.base_branch))
        }
        EventPayload::Node(NodeEvent::RunnerResolved(p)) => Some(format!(
            "{} on {}/{}",
            p.runner, p.chosen.adapter, p.chosen.model
        )),
        EventPayload::Node(NodeEvent::Started(p)) => Some(format!("attempt {}", p.attempt)),
        EventPayload::Session(SessionEvent::Message(p)) => p
            .tool_name
            .as_ref()
            .map(|tool| format!("{:?} {tool}", p.message_type)),
        EventPayload::Artifacts(ArtifactEvent::Written(p)) => Some(p.path.display().to_string()),
        EventPayload::Tasks(TaskEvent::Registered(p)) => Some(p.task_id.to_string()),
        EventPayload::Tasks(TaskEvent::StatusChanged(p)) => {
            Some(format!("{} is {:?}", p.task_id, p.new_status))
        }
        EventPayload::Node(NodeEvent::CriteriaChecked(p)) => Some(format!(
            "{} {:?}: {} criteria",
            p.task_id,
            p.phase,
            p.results.len()
        )),
        EventPayload::Node(NodeEvent::ScopeChecked(p)) => Some(format!(
            "{} out of scope",
            yunta_core::text::counted(p.violations.len(), "path")
        )),
        EventPayload::Node(NodeEvent::Finished(p)) => Some(p.outcome.clone()),
        EventPayload::Node(NodeEvent::Failed(p)) => Some(p.failure.to_string()),
        EventPayload::Node(NodeEvent::HookExecuted(p)) => {
            Some(format!("{:?} exit {}", p.phase, p.exit_code))
        }
        EventPayload::Node(NodeEvent::Rerouted(p)) => {
            Some(detailed(format!("to `{}`", p.to_node), &p.cause))
        }
        EventPayload::Gates(GateEvent::Waiting(p)) => Some(p.summary().to_string()),
        EventPayload::Gates(GateEvent::Resolved(p)) => Some(resolution(p)),
        EventPayload::Children(ChildEvent::LoopIteration(p)) => {
            Some(format!("iteration {}", p.iteration))
        }
        // What a session did to a finding after posting it, and what
        // the engine answered when it refused the call.
        EventPayload::Findings(FindingEvent::Updated(p)) => Some(detailed(
            format!("{:?}", p.finding.severity),
            &p.finding.title,
        )),
        EventPayload::Findings(FindingEvent::Withdrawn(p)) => {
            Some(detailed(format!("`{}`", p.id), &p.reason))
        }
        EventPayload::Findings(FindingEvent::Refused(p)) => Some(detailed(
            match &p.id {
                Some(id) => format!("{:?} `{id}`", p.operation),
                None => format!("{:?}", p.operation),
            },
            &yunta_core::text::counted(p.report.diagnostics.len(), "problem"),
        )),
        // A document a session offered as a whole, and whether the
        // engine took it.
        EventPayload::Artifacts(ArtifactEvent::Submitted(p)) => Some(detailed(
            format!("{:?} {}", p.artifact_kind, p.name),
            match &p.outcome {
                yunta_core::events::SubmissionOutcome::Accepted { .. } => "accepted",
                yunta_core::events::SubmissionOutcome::Refused { .. } => "refused",
            },
        )),
        EventPayload::Artifacts(ArtifactEvent::Accepted(p)) => Some(format!("{}", p.artifact)),
        EventPayload::Findings(FindingEvent::Posted(p)) => Some(detailed(
            format!("{:?}", p.finding.severity),
            &p.finding.title,
        )),
        EventPayload::Run(RunEvent::PromotionSignaled(p)) => {
            Some(detailed(format!("to `{}`", p.suggested_mode), &p.reason))
        }
        EventPayload::Children(ChildEvent::Created(p)) => Some(p.child_run_id.to_string()),
        EventPayload::Children(ChildEvent::Finished(p)) => Some(format!(
            "{} {}",
            p.child_run_id,
            view::closed_as(p.terminal_state)
        )),
        EventPayload::Session(SessionEvent::CapabilityDegraded(p)) => Some(detailed(
            format!("{} on {}", p.capability.as_str(), p.adapter),
            p.policy_applied(),
        )),
        // A write that did not happen, and what it would have touched.
        EventPayload::Session(SessionEvent::WriteRefused(p)) => {
            Some(format!("write refused: {}", p.target.sentence()))
        }
        EventPayload::Run(RunEvent::Paused(p)) => Some(p.reason().to_string()),
        EventPayload::Run(RunEvent::Finished(p)) => {
            Some(view::closed_as(p.terminal_state).to_string())
        }
        // A node that asked: what it asked, so a reader knows what the
        // run is waiting on without opening the document.
        EventPayload::Gates(GateEvent::QuestionsAsked(p)) => Some(format!(
            "asked {}: {}",
            yunta_core::text::counted(p.questions.len(), "question"),
            p.questions
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
        // How much of the session the adapter's fence covered — what
        // says whether a write that reaches the diff should have been
        // possible at all.
        EventPayload::Session(SessionEvent::Opened(p)) => p.fence.as_ref().map(fence_covered),
        EventPayload::Node(NodeEvent::BaselineCaptured(_))
        | EventPayload::Node(NodeEvent::ContextAssembled(_))
        | EventPayload::Scope(ScopeEvent::Requested(_))
        | EventPayload::Scope(ScopeEvent::Granted(_))
        | EventPayload::Scope(ScopeEvent::Denied(_))
        | EventPayload::Gates(GateEvent::QuestionsAnswered(_))
        | EventPayload::Run(RunEvent::Resumed(_)) => None,
    }
}

/// What a session's fence covered, in one phrase.
fn fence_covered(coverage: &Coverage) -> String {
    match coverage {
        Coverage::Exact => "fence exact".to_string(),
        Coverage::WidenedToRoots { roots } => {
            format!(
                "fence widened to {}",
                yunta_core::text::counted(roots.len(), "root")
            )
        }
        Coverage::ToolsOnly => "fence on tool calls".to_string(),
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
        EventPayload, Evidence, Failure, Finding, FindingPostedPayload, FindingSeverity,
        NodeFinishedPayload, NodeReroutedPayload, PromotionSignaledPayload, RerouteCause,
        RerouteOrigin, RunPausedPayload, StoredEvent, TokenUsage,
    };
    use yunta_core::events::{FindingEvent, NodeEvent, RunEvent};

    use super::{detail, Lines};

    /// The line one event writes, as a reader meets it: the surface's
    /// own output, not the private field it is built from.
    fn line_for(payload: EventPayload) -> String {
        let captured = yunta_testkit_core::Captured::default();
        let mut lines = Lines::open(Box::new(captured.clone()), "a test is reading");
        lines.event(&StoredEvent {
            seq: yunta_core::Seq::from(1),
            run_id: "01JQ0000000000000000000000".into(),
            node_id: None,
            timestamp: yunta_core::Clock::now(&yunta_testkit_core::FixedClock),
            body: yunta_core::events::EventBody::Known(payload),
        });
        captured
            .text()
            .lines()
            .last()
            .unwrap_or_default()
            .to_string()
    }

    /// A log this binary reads back was written by some other
    /// invocation: nothing guarantees the free text on a payload says
    /// anything, and a line built for it must not promise that it does.
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
        let payload = EventPayload::Run(RunEvent::PromotionSignaled(PromotionSignaledPayload {
            reason: String::new(),
            evidence: Evidence::none(),
            suggested_mode: "ship".into(),
        }));
        assert_eq!(detail(&payload).as_deref(), Some("to `ship`"));
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
        assert_eq!(detail(&payload).as_deref(), Some("Minor"));
    }

    #[test]
    fn a_pause_with_no_reason_recorded_leaves_the_line_at_its_kind() {
        let line = line_for(EventPayload::Run(RunEvent::Paused(
            RunPausedPayload::recorded("   ".to_string()),
        )));
        assert_eq!(
            line, "[0s] run_paused",
            "no dash promises what is not there"
        );
    }

    #[test]
    fn a_pause_that_recorded_a_reason_says_it_on_one_line() {
        let line = line_for(EventPayload::Run(RunEvent::Paused(
            RunPausedPayload::recorded("budget\n  reached".to_string()),
        )));
        assert_eq!(line, "[0s] run_paused — budget reached");
    }

    #[test]
    fn a_node_that_finished_saying_nothing_leaves_the_line_at_its_kind() {
        let line = line_for(EventPayload::Node(NodeEvent::Finished(
            NodeFinishedPayload::new(String::new(), TokenUsage::default()),
        )));
        assert_eq!(line, "[0s] node_finished");
    }

    /// A log written before the engine named its fallbacks carries an
    /// empty `policy_applied`. The line states what was missing and
    /// claims nothing about what was done instead.
    #[test]
    fn a_degraded_capability_with_no_policy_recorded_reads_as_what_was_missing() {
        let payload: EventPayload = serde_json::from_value(serde_json::json!({
            "kind": "capability_degraded",
            "capability": "run_tools",
            "adapter": "codex",
            "policy_applied": "",
        }))
        .expect("the wire form of a degradation with no policy");
        assert_eq!(detail(&payload).as_deref(), Some("run_tools on codex"));
    }
}

/// The one line a parked run shows, produced once and read by every
/// surface that has to show it.
///
/// `PauseReason` is the fact; its `Display` is the only prose. This
/// checks the three places a reader meets it — the chronicle's own line,
/// the closing verdict, and the page a `status` prints — say the same
/// thing, for every reason a run can park on.
#[cfg(test)]
mod pause_reason_tests {
    use yunta_core::events::{Escalation, Fact, Failure, PauseReason, RunEvent, RunPausedPayload};
    use yunta_core::{NodeId, QuestionId};
    use yunta_engine::WaitingOn;

    use crate::commands::advice;

    use super::{detail, EventPayload};

    /// One of every reason a run parks on. The `match` is what keeps it
    /// complete: a new variant stops compiling here until it is listed.
    fn every_reason() -> Vec<PauseReason> {
        let all = vec![
            PauseReason::Escalation(Box::new(
                Escalation::published_to(
                    "Ready to open the PR?",
                    vec![Fact::labelled("published at", "example/repo#1")].into(),
                    "https://example.invalid/pull/1",
                )
                .expect("the summary states no fact the evidence holds"),
            )),
            PauseReason::Cancelled,
            PauseReason::CancelledAfterCrash,
            PauseReason::BudgetExhausted {
                spent: 120_000,
                cap: 100_000,
            },
            PauseReason::LoopOverrun {
                node: NodeId::from("build"),
                cap: 8,
            },
            PauseReason::ExternalGate {
                url: "https://example.invalid/pull/1".to_string(),
            },
            PauseReason::UncertainOrphans(vec![NodeId::from("review")]),
            PauseReason::NodeFailed {
                node: NodeId::from("lint"),
                failure: Failure::message("exit 1"),
            },
            PauseReason::Blocked {
                node: NodeId::from("ship"),
                on: vec![NodeId::from("lint")],
            },
            PauseReason::GateAborted {
                node: NodeId::from("approve"),
                free_text: Some("not this quarter".to_string()),
            },
            PauseReason::ChildPaused {
                node: NodeId::from("fanout"),
                reason: "cancelled by user".to_string(),
            },
            PauseReason::Questions {
                node: NodeId::from("scope"),
                pending: (QuestionId::from("q1"), Vec::new()).into(),
            },
            PauseReason::AnswersRefused {
                node: NodeId::from("scope"),
                report: yunta_core::diagnostic::Report::new(
                    yunta_core::diagnostic::DocumentRef::new(
                        yunta_core::ArtifactKind::Answers,
                        "answers.yaml",
                    ),
                    vec![yunta_core::diagnostic::Diagnostic::new(
                        yunta_core::diagnostic::Subject::Question(
                            yunta_core::diagnostic::Named::new(
                                yunta_core::QuestionId::from("q2"),
                                0,
                            ),
                        ),
                        yunta_core::diagnostic::Problem::rule(
                            yunta_core::diagnostic::RuleCode::MissingAnswer,
                            "this question is `required` and nothing answers it",
                        ),
                    )],
                ),
            },
        ];
        for reason in &all {
            match reason {
                PauseReason::Escalation(_)
                | PauseReason::Cancelled
                | PauseReason::CancelledAfterCrash
                | PauseReason::BudgetExhausted { .. }
                | PauseReason::LoopOverrun { .. }
                | PauseReason::ExternalGate { .. }
                | PauseReason::UncertainOrphans(_)
                | PauseReason::NodeFailed { .. }
                | PauseReason::Blocked { .. }
                | PauseReason::GateAborted { .. }
                | PauseReason::ChildPaused { .. }
                | PauseReason::Questions { .. }
                | PauseReason::AnswersRefused { .. } => {}
            }
        }
        all
    }

    #[test]
    fn a_pause_reason_renders_once_at_the_border() {
        for reason in every_reason() {
            let sentence = reason.to_string();
            assert!(
                !sentence.is_empty(),
                "a reason a reader is shown says something"
            );
            let payload = RunPausedPayload::new(&reason);
            assert_eq!(payload.reason(), sentence);

            let chronicle = detail(&EventPayload::Run(RunEvent::Paused(payload.clone())));
            assert_eq!(chronicle.as_deref(), Some(sentence.as_str()));

            let waiting = WaitingOn::Run {
                reason: payload.reason().to_string(),
            };
            let one_line = yunta_core::text::one_line(&sentence);
            assert_eq!(advice::parked_on(&waiting), one_line);
            assert_eq!(advice::parked_in_full(&waiting), one_line);
        }
    }
}
