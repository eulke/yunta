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
use yunta_core::events::{EventPayload, GateResolvedPayload, StoredEvent, TerminalState};

use crate::render::format_duration;

use super::write_line;

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

    /// Writes `event`'s line.
    pub(super) fn event(&mut self, event: &StoredEvent) {
        let opened = *self.opened.get_or_insert(event.timestamp);
        let elapsed = (event.timestamp - opened).to_std().unwrap_or_default();
        let mut line = format!("[{}] {}", format_duration(elapsed), event.body.kind_name());
        if let Some(node) = &event.node_id {
            line.push_str(&format!(" on `{node}`"));
        }
        if let Some(detail) = event.payload().and_then(detail) {
            line.push_str(&format!(" — {}", yunta_core::text::one_line(&detail)));
        }
        write_line(&mut self.out, &line);
    }
}

/// What one event carries beyond its kind and its node: the fact a reader
/// scanning the log acts on.
///
/// `None` where the kind says everything it has to say — the event still
/// gets its line, because a kind this binary cannot detail is still an
/// event that happened, and one it does not know at all still has a name.
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
        EventPayload::NodeFinished(p) => Some(p.outcome.clone()),
        EventPayload::NodeFailed(p) => Some(p.failure.to_string()),
        EventPayload::HookExecuted(p) => Some(format!("{:?} exit {}", p.phase, p.exit_code)),
        EventPayload::NodeRerouted(p) => Some(format!("to `{}`: {}", p.to_node, p.cause)),
        EventPayload::GateWaiting(p) => Some(p.summary.clone()),
        EventPayload::GateResolved(p) => Some(resolution(p)),
        EventPayload::LoopIteration(p) => Some(format!("iteration {}", p.iteration)),
        EventPayload::FindingPosted(p) => {
            Some(format!("{:?}: {}", p.finding.severity, p.finding.title))
        }
        EventPayload::PromotionSignaled(p) => {
            Some(format!("to `{}`: {}", p.suggested_mode, p.reason))
        }
        EventPayload::ChildRunCreated(p) => Some(p.child_run_id.to_string()),
        EventPayload::ChildRunFinished(p) => {
            Some(format!("{} {}", p.child_run_id, terminal(p.terminal_state)))
        }
        EventPayload::CapabilityDegraded(p) => Some(format!(
            "{:?} on {}: {}",
            p.capability, p.adapter, p.policy_applied
        )),
        EventPayload::RunPaused(p) => Some(p.reason.clone()),
        EventPayload::RunFinished(p) => Some(terminal(p.terminal_state).to_string()),
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

/// The word a run's terminal state gets on this surface, so the line
/// that closes a run reads as prose rather than as the name of a variant.
fn terminal(state: TerminalState) -> &'static str {
    match state {
        TerminalState::Done => "finished",
        TerminalState::Failed => "failed",
        TerminalState::Cancelled => "cancelled",
        TerminalState::Promoted => "promoted",
    }
}
