//! The versioned JSON the CLI emits for machines: `stats --json`,
//! `status --json`, `run --json`, and the same DTOs the `yunta mcp`
//! control plane returns in a tool result. One schema version spans
//! them, so a reader keys off a single number that changes only when a
//! field's meaning does — the CLI's own text output is for people, this
//! is the contract for programs.
//!
//! A run is one document here, [`RunDocument`], whichever command
//! produced it: `yunta run` reporting the stop it drove to, `yunta
//! resume` reporting the stop it picked up, `yunta status` reporting
//! where a run stands right now. They are one document because they are
//! one question, and a program that learned to read the answer from one
//! command reads it from the next.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use yunta_core::events::{ArtifactId, EventPayload, Failure, NodeEvent, StoredEvent};
use yunta_core::{ArtifactFailure, Diagnostic, FileProblem, Manifest, NodeId, RunId};
use yunta_engine::{RunPhase, WaitingOn};

use crate::commands::status::{decision, progress, task_status_label};
use crate::error::{CliError, Outcome};
use crate::render::state::RunWord;
use crate::render::NodeDisplay;

/// The version stamped on every machine-readable document this CLI emits.
/// Bumped when a field's meaning changes, never for an additive one, so a
/// reader can refuse a document from a schema it predates.
///
/// One number covers every `--json` document together, so a bump earned
/// by one of them re-stamps all of them: a document whose own shape did
/// not change still carries the new number.
pub const SCHEMA_VERSION: u32 = 4;

/// Serializes a DTO as pretty JSON to stdout — the one place a `--json`
/// command prints its document, reporting a serialization failure as the
/// one error it can hit rather than unwrapping it.
pub fn print_json<T: serde::Serialize>(value: &T) -> Result<Outcome, CliError> {
    println!("{}", to_json_string(value).map_err(CliError::msg)?);
    Ok(Outcome::Success)
}

/// Serializes a DTO as pretty JSON to a string — what the control plane
/// hands back in a tool result instead of printing, so a `println!` never
/// lands in the middle of its stdio JSON-RPC stream.
pub fn to_json_string<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value)
        .map_err(|e| format!("could not serialize output as JSON: {e}"))
}

/// Where a run stands, as the one versioned document every surface that
/// reports a run emits: `yunta run --json`, `yunta resume --json`,
/// `yunta status --json`, and the `workflow_status` control-plane tool.
///
/// It is derived from the run's own event log and nothing else, so the
/// command that drove a run to its stop and the command that reads that
/// run an hour later publish the same answer. `outcome` is the word
/// every text surface prints, spelled the same way here: a reader who
/// saw `paused` on a terminal finds `paused` in the document.
#[derive(serde::Serialize)]
pub(crate) struct RunDocument {
    schema_version: u32,
    run_id: String,
    /// How the run stands, in the one vocabulary every surface uses.
    outcome: RunWord,
    /// The same standing in a sentence, with the counters a person reads
    /// beside it.
    summary: String,
    /// Why the run stopped where it did, when `outcome` alone does not
    /// say it. A parked run says it under `waiting_on` instead, and a
    /// finished one has nothing to add.
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    /// The one piece of the pre-run estimation §8.6 of the run contract
    /// makes actionable: the declared cap sits under what this workflow
    /// has historically spent. Carried only by the invocation that
    /// *created* the run, which is the only one that has it — a reader
    /// of the log alone never does.
    #[serde(skip_serializing_if = "Option::is_none")]
    budget_warning: Option<String>,
    nodes: BTreeMap<String, String>,
    tasks: BTreeMap<String, &'static str>,
    /// Why each failed node failed, in the form a program can act on
    /// rather than parse back out of a sentence: one entry per declared
    /// artifact that did not close, carrying what went wrong with it.
    /// Absent when no failing node named an artifact.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    diagnostics: BTreeMap<String, Vec<ArtifactProblems>>,
    /// The decision this run is parked on: the node it belongs to, the
    /// escalation under the field names the `gate_waiting` event writes,
    /// and the command that answers it.
    ///
    /// Absent for a run that is not parked, and for a pause that
    /// reconstructs no menu — a budget cap, a scope expansion, an
    /// unanswered questions artifact, an external gate with no reachable
    /// forge. Those pauses are answered where they were raised, and
    /// `waiting_on` carries what the run is waiting on.
    #[serde(skip_serializing_if = "Option::is_none")]
    decision: Option<decision::DecisionJson>,
    /// What the run is parked on, for every parked run — including the
    /// pauses `decision` is absent for. `summary` says it in a sentence;
    /// this says it in a shape a program acts on without parsing one.
    #[serde(skip_serializing_if = "Option::is_none")]
    waiting_on: Option<WaitingOnJson>,
    tokens: TokensJson,
}

impl RunDocument {
    /// The run as its own log describes it at `now`, which the caller's
    /// injected clock decides.
    pub(crate) fn of(
        run_id: &RunId,
        events: &[StoredEvent],
        manifest: &Manifest,
        now: DateTime<Utc>,
    ) -> Self {
        let state = yunta_engine::derive(events);
        let frame = progress::frame(run_id, manifest, events, now);
        RunDocument {
            schema_version: SCHEMA_VERSION,
            run_id: run_id.to_string(),
            outcome: RunWord::of(&frame.phase),
            summary: progress::summary(&frame),
            reason: reason(&frame.phase),
            budget_warning: None,
            nodes: state
                .nodes
                .iter()
                .map(|(id, node)| (id.to_string(), NodeDisplay::of(node.state.as_ref()).label()))
                .collect(),
            tasks: state
                .tasks
                .iter()
                .map(|(id, record)| (id.to_string(), task_status_label(record.status)))
                .collect(),
            diagnostics: node_diagnostics(events),
            decision: parked_decision(run_id, manifest, events, &frame.phase),
            waiting_on: WaitingOnJson::of(&frame.phase),
            tokens: TokensJson {
                input: state.total_tokens().input,
                output: state.total_tokens().output,
            },
        }
    }

    /// The same document carrying the pre-run estimation's warning.
    pub(crate) fn warning(mut self, budget_warning: Option<String>) -> Self {
        self.budget_warning = budget_warning;
        self
    }

    /// The word this document reports, for a caller that turns it into
    /// the invocation's own verdict.
    pub(crate) fn outcome(&self) -> RunWord {
        self.outcome
    }
}

/// Why the run stopped where it did, for the phases whose word alone
/// does not say it: what broke a failed run, and what stopped making
/// sense in a broken one.
fn reason(phase: &RunPhase) -> Option<String> {
    match phase {
        RunPhase::Failed { failure } => failure
            .as_ref()
            .map(|failure| yunta_core::text::one_line(&failure.to_string())),
        RunPhase::Broken { diagnostic } => Some(yunta_core::text::one_line(diagnostic)),
        RunPhase::Created
        | RunPhase::Running
        | RunPhase::Waiting { .. }
        | RunPhase::Finished
        | RunPhase::Cancelled
        | RunPhase::Promoted { .. } => None,
    }
}

/// The decision a parked run waits on, for a program to read: the same
/// escalation the text view prints, from the same reconstruction, so a
/// client that answers through `resolve_gate` chooses from exactly the
/// menu a person sees. `None` for a run that is not parked and for a
/// pause that reconstructs no menu.
fn parked_decision(
    run_id: &RunId,
    manifest: &Manifest,
    events: &[StoredEvent],
    phase: &RunPhase,
) -> Option<decision::DecisionJson> {
    if !matches!(phase, RunPhase::Waiting { .. }) {
        return None;
    }
    let (node, escalation) =
        yunta_engine::current_escalation(manifest, &yunta_engine::derive(events))?;
    Some(decision::DecisionJson::new(
        run_id,
        &node,
        escalation.into_payload(),
    ))
}

/// What a waiting run is waiting on.
///
/// Tagged by `on`, so a reader matches on the shape instead of
/// inferring it from which fields are set: a node parked on a person is
/// a different thing from the run itself stopping, and only one of them
/// has a node to name.
#[derive(serde::Serialize)]
#[serde(tag = "on", rename_all = "snake_case")]
pub(crate) enum WaitingOnJson {
    /// A node is parked on a person. With several parked at once this
    /// is the first in the workflow's declaration order.
    Node {
        node: String,
        /// The forge's own handle for a published gate.
        #[serde(skip_serializing_if = "Option::is_none")]
        external_ref: Option<String>,
        /// What the run's standing pause recorded, absent for a node
        /// parked while the run itself keeps moving.
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// The run itself paused, with the reason it recorded.
    Run { reason: String },
}

impl WaitingOnJson {
    /// The frame's own answer, in the document's shape. `None` for a run
    /// that is not waiting on anything.
    fn of(phase: &RunPhase) -> Option<Self> {
        let RunPhase::Waiting { on } = phase else {
            return None;
        };
        Some(match on {
            WaitingOn::Node {
                node,
                external_ref,
                reason,
            } => WaitingOnJson::Node {
                node: node.to_string(),
                external_ref: external_ref.clone(),
                reason: reason.clone(),
            },
            WaitingOn::Run { reason } => WaitingOnJson::Run {
                reason: reason.clone(),
            },
        })
    }
}

#[derive(serde::Serialize)]
struct TokensJson {
    input: u64,
    output: u64,
}

/// One artifact a node's failure names, with what is wrong with it. A
/// node that failed on two artifacts produces two of these, so a reader
/// attributes a problem to the artifact it came from instead of
/// splitting a sentence.
///
/// Every field is absent when the failure has nothing to put there, so
/// no consumer ever meets an invented path or an empty code: an artifact
/// another run owes and a document its node never handed over have no
/// file to open and no content that was read, and a file that was never
/// written has no kind its content could have met.
#[derive(Default, serde::Serialize)]
pub(crate) struct ArtifactProblems {
    /// The stable name of what is wrong with the artifact itself —
    /// `artifact-missing`, `artifact-undelivered`, `artifact-unheld`.
    /// Absent when the file is there and its content is what failed,
    /// because that is not one problem: each of the document's own
    /// carries its code.
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<yunta_core::diagnostic::DiagnosticCode>,
    /// As a reader would type it to open the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    /// The kind whose shape the content had to meet.
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<yunta_core::diagnostic::DocumentKind>,
    /// What is wrong with the file itself — never written, empty, past
    /// the ceiling, refused by the filesystem.
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<FileProblem>,
    /// The run that owes this artifact and holds none of it — where a
    /// reader goes to look. Absent when this run owes it, which is what
    /// a document its own node never handed over is.
    #[serde(skip_serializing_if = "Option::is_none")]
    run: Option<String>,
    /// The node the artifact was asked of: of `run` when one is named,
    /// of this run otherwise. Absent when the question is about a run as
    /// a whole.
    #[serde(skip_serializing_if = "Option::is_none")]
    producer: Option<String>,
    /// The identity that was asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    artifact: Option<ArtifactId>,
    /// Every problem this document's content has, in document order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    diagnostics: Vec<Diagnostic>,
}

impl From<&ArtifactFailure> for ArtifactProblems {
    fn from(failure: &ArtifactFailure) -> Self {
        let code = failure.code();
        match failure {
            ArtifactFailure::File { path, problem } => ArtifactProblems {
                code,
                path: Some(path.clone()),
                file: Some(problem.clone()),
                ..ArtifactProblems::default()
            },
            ArtifactFailure::Undelivered { node, artifact } => ArtifactProblems {
                code,
                producer: Some(node.to_string()),
                artifact: Some(artifact.clone()),
                ..ArtifactProblems::default()
            },
            ArtifactFailure::Content(report) => ArtifactProblems {
                path: Some(report.document.path.clone()),
                kind: Some(report.document.kind),
                diagnostics: report.diagnostics.clone(),
                ..ArtifactProblems::default()
            },
            ArtifactFailure::Unheld {
                run,
                producer,
                artifact,
            } => ArtifactProblems {
                code,
                run: Some(run.to_string()),
                producer: producer.as_ref().map(NodeId::to_string),
                artifact: Some(artifact.clone()),
                ..ArtifactProblems::default()
            },
        }
    }
}

/// The artifacts each node's most recent failure names, with what is
/// wrong with each, keyed by node.
///
/// An entry describes the failure a node is in **now**, never every
/// failure it has had: a node that failed, repaired and failed again is
/// described by its last failure, and one that started again, or whose
/// latest failure is a sentence naming no document, has no entry at
/// all. A reader asking what is wrong now is not asking for a history.
fn node_diagnostics(events: &[StoredEvent]) -> BTreeMap<String, Vec<ArtifactProblems>> {
    let mut latest = BTreeMap::new();
    for event in events {
        let Some(node_id) = event.node_id.as_ref() else {
            continue;
        };
        match event.payload() {
            Some(EventPayload::Node(NodeEvent::Failed(p))) => match &p.failure {
                Failure::Artifacts { artifacts } => {
                    latest.insert(
                        node_id.to_string(),
                        artifacts.iter().map(ArtifactProblems::from).collect(),
                    );
                }
                // A failure stated in one sentence names no document;
                // the node's own line carries it whole.
                Failure::Message { .. } => {
                    latest.remove(node_id.as_str());
                }
            },
            // A node that started again has left its last failure behind.
            Some(EventPayload::Node(NodeEvent::Started(_))) => {
                latest.remove(node_id.as_str());
            }
            _ => {}
        }
    }
    latest
}
