//! `yunta status <run_id>`: where a run stands, derived from the event
//! log alone, never from an estimate or an agent's own report. Two
//! levels — **flow** (the DAG's nodes, over the ones this run's mode
//! schedules) and **task** (ledger tasks done/total) — presented as
//! counters with context, never a percentage: a percentage lies the
//! moment a reroute grows the denominator.
//!
//! A run parked on a person is answerable from here: the decision it
//! stopped on is rebuilt from its own log ([`decision`]) and printed
//! with the command that answers it, so nobody has to read the exported
//! JSONL to learn what the options are.

pub(crate) mod decision;
pub(crate) mod progress;

use chrono::{DateTime, Utc};

use yunta_core::events::{EventPayload, Failure, StoredEvent, TaskStatus};
use yunta_core::{ArtifactFailure, ArtifactKind, Clock, Diagnostic, FileProblem};
use yunta_core::{Manifest, NodeId, RunId};
use yunta_engine::{NodeState, RunPhase};

use crate::commands::advice;
use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::load_yaml;
use crate::render::{indent, NodeDisplay, INDENT};

pub fn status(run_id: &RunId, json: bool) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let storage = ctx.storage()?;
    let events = storage.events_for_run(run_id)?;
    if events.is_empty() {
        return Err(CliError::msg(format!(
            "no run `{run_id}` in {}",
            ctx.project.storage_path.display()
        )));
    }

    // Search order (current runs root, then the default) — the run's
    // own frozen paths take over once the manifest is open.
    let manifest_path = ctx
        .project
        .run_dir(run_id.as_str())
        .unwrap_or_else(|| ctx.project.runs_root.join(run_id.as_str()))
        .join("manifest.yaml");
    let manifest: Manifest = load_yaml(&manifest_path, "run manifest")?;

    let now = ctx.clock.now();
    if json {
        return crate::json::print_json(&status_json(run_id, &events, &manifest, now));
    }

    let frame = progress::frame(run_id, &manifest, &events, now);
    println!("run {run_id}: {}", progress::summary(&frame));
    print_derived(&yunta_engine::derive(&events));
    print_decision(run_id, &manifest, &events, &frame.phase);
    Ok(Outcome::Success)
}

/// The detail under the summary: every node and every task by its own
/// derived state, what each failure names, and what the run has spent.
fn print_derived(state: &yunta_engine::RunState) {
    if !state.nodes.is_empty() {
        println!("nodes:");
        let mut nodes: Vec<_> = state.nodes.iter().collect();
        nodes.sort_by(|a, b| a.0.cmp(b.0));
        for (id, node) in nodes {
            println!("{INDENT}{id}: {}", NodeDisplay::of(Some(node)).label());
        }
    }

    if !state.tasks.is_empty() {
        println!("tasks:");
        let mut tasks: Vec<_> = state.tasks.iter().collect();
        tasks.sort_by(|a, b| a.0.cmp(b.0));
        for (id, status) in tasks {
            println!("{INDENT}{id}: {}", task_status_label(status));
        }
    }

    print_failures(state);

    println!(
        "tokens: {} in / {} out",
        state.total_tokens.input, state.total_tokens.output
    );
}

/// What a parked run is waiting on, printed last because it is what the
/// reader acts on next: the decision the log reconstructs, with the
/// command that answers it, or — for a pause that reconstructs none —
/// the sentence the frame carries for it and the way back into the run.
/// A run that is not parked prints nothing here.
fn print_decision(run_id: &RunId, manifest: &Manifest, events: &[StoredEvent], phase: &RunPhase) {
    let Some(waiting) = advice::parked(phase) else {
        return;
    };
    match yunta_engine::current_escalation(manifest, events) {
        Some((node, escalation)) => print!(
            "{}",
            decision::block(decision::Layout::Page, run_id, &node, &escalation)
        ),
        None => print!(
            "{}",
            decision::without_menu(run_id, &advice::parked_in_full(waiting))
        ),
    }
}

/// Every failure with more than one line of detail, laid out one block
/// per failing document: the path a reader opens, then that document's
/// own problems under it.
///
/// The node list above stays scannable at one line each, which means
/// collapsing them there. Doing only that would leave the one surface a
/// person opens to find out what went wrong unable to say. A node that
/// failed on two artifacts says which problem came from which, because
/// the log records each document's problems with the document.
fn print_failures(state: &yunta_engine::RunState) {
    let mut failed: Vec<(&NodeId, &Failure)> = state
        .nodes
        .iter()
        .filter_map(|(id, node)| match node {
            NodeState::Failed { failure, .. } => Some((id, failure)),
            _ => None,
        })
        .filter(|(_, failure)| match failure {
            Failure::Artifacts { artifacts } => !artifacts.is_empty(),
            Failure::Message { outcome } => outcome.contains('\n'),
        })
        .collect();
    if failed.is_empty() {
        return;
    }
    failed.sort_by(|a, b| a.0.cmp(b.0));
    println!("failures:");
    // A document's problems hang under the node that named it, which is
    // itself one step under the heading.
    let detail = indent(2);
    for (id, failure) in failed {
        println!("{INDENT}{id}:");
        match failure {
            Failure::Artifacts { artifacts } => {
                for artifact in artifacts {
                    println!(
                        "{}",
                        yunta_core::text::indent(&artifact.to_string(), &detail)
                    );
                }
            }
            Failure::Message { outcome } => {
                println!("{}", yunta_core::text::indent(outcome, &detail));
            }
        }
    }
}

/// A run's derived state as the versioned JSON `yunta status --json`
/// prints and the `workflow_status` control-plane tool returns — one DTO,
/// so a machine reads the same shape from either surface.
#[derive(serde::Serialize)]
pub(crate) struct StatusJson {
    schema_version: u32,
    run_id: String,
    summary: String,
    nodes: std::collections::BTreeMap<String, String>,
    tasks: std::collections::BTreeMap<String, &'static str>,
    /// Why each failed node failed, in the form a program can act on
    /// rather than parse back out of a sentence: one entry per document
    /// the failure names, carrying that document's own problems. Absent
    /// when no failing node named a document.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    diagnostics: std::collections::BTreeMap<String, Vec<DocumentProblems>>,
    /// The decision this run is parked on: the node it belongs to, the
    /// escalation under the field names the `gate_waiting` event writes,
    /// and the command that answers it.
    ///
    /// Absent for a run that is not parked, and for a pause that
    /// reconstructs no menu — a budget cap, a scope expansion, an
    /// unanswered questions artifact, an external gate with no reachable
    /// forge. Those pauses are answered where they were raised, and
    /// `summary` carries what the run is waiting on.
    #[serde(skip_serializing_if = "Option::is_none")]
    decision: Option<decision::DecisionJson>,
    tokens: TokensJson,
}

/// One document a node's failure names, with the problems that belong
/// to it. A node that failed on two artifacts produces two of these, so
/// a reader attributes a problem to the file it came from instead of
/// splitting a sentence.
#[derive(serde::Serialize)]
pub(crate) struct DocumentProblems {
    /// As a reader would type it to open the file.
    path: String,
    /// The kind whose shape the content had to meet. Absent when the
    /// file itself is what failed, because nothing read its content.
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<ArtifactKind>,
    /// What is wrong with the file itself — never written, empty, past
    /// the ceiling, refused by the filesystem. Absent when the file is
    /// there and its content is what failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<FileProblem>,
    /// Every problem this document's content has, in document order.
    /// Empty when the file itself is what failed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    diagnostics: Vec<Diagnostic>,
}

impl From<&ArtifactFailure> for DocumentProblems {
    fn from(failure: &ArtifactFailure) -> Self {
        match failure {
            ArtifactFailure::File { path, problem } => DocumentProblems {
                path: path.clone(),
                kind: None,
                file: Some(problem.clone()),
                diagnostics: Vec::new(),
            },
            ArtifactFailure::Content(report) => DocumentProblems {
                path: report.document.path.clone(),
                kind: Some(report.document.kind),
                file: None,
                diagnostics: report.diagnostics.clone(),
            },
        }
    }
}

#[derive(serde::Serialize)]
struct TokensJson {
    input: u64,
    output: u64,
}

/// Derives a run's [`StatusJson`] from its event log, its frozen manifest
/// and the instant the caller reads it at — the print-free core
/// `status --json` and the control plane share, so a program reads the
/// same document whichever surface it asks. `now` comes from the caller's
/// injected clock.
pub(crate) fn status_json(
    run_id: &RunId,
    events: &[StoredEvent],
    manifest: &Manifest,
    now: DateTime<Utc>,
) -> StatusJson {
    let state = yunta_engine::derive(events);
    let frame = progress::frame(run_id, manifest, events, now);
    StatusJson {
        schema_version: crate::json::SCHEMA_VERSION,
        run_id: run_id.to_string(),
        summary: progress::summary(&frame),
        nodes: state
            .nodes
            .iter()
            .map(|(id, node)| (id.to_string(), NodeDisplay::of(Some(node)).label()))
            .collect(),
        tasks: state
            .tasks
            .iter()
            .map(|(id, status)| (id.to_string(), task_status_label(status)))
            .collect(),
        diagnostics: node_diagnostics(events),
        decision: parked_decision(run_id, manifest, events, &frame.phase),
        tokens: TokensJson {
            input: state.total_tokens.input,
            output: state.total_tokens.output,
        },
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
    let (node, escalation) = yunta_engine::current_escalation(manifest, events)?;
    Some(decision::DecisionJson::new(run_id, &node, escalation))
}

/// The documents each node's most recent failure names, with their
/// problems, keyed by node.
///
/// An entry describes the failure a node is in **now**, never every
/// failure it has had: a node that failed, repaired and failed again is
/// described by its last failure, and one that started again, or whose
/// latest failure is a sentence naming no document, has no entry at
/// all. A reader asking what is wrong now is not asking for a history.
fn node_diagnostics(
    events: &[StoredEvent],
) -> std::collections::BTreeMap<String, Vec<DocumentProblems>> {
    let mut latest = std::collections::BTreeMap::new();
    for event in events {
        let Some(node_id) = event.node_id.as_ref() else {
            continue;
        };
        match event.payload() {
            Some(EventPayload::NodeFailed(p)) => match &p.failure {
                Failure::Artifacts { artifacts } => {
                    latest.insert(
                        node_id.to_string(),
                        artifacts.iter().map(DocumentProblems::from).collect(),
                    );
                }
                // A failure stated in one sentence names no document;
                // the node's own line carries it whole.
                Failure::Message { .. } => {
                    latest.remove(node_id.as_str());
                }
            },
            // A node that started again has left its last failure behind.
            Some(EventPayload::NodeStarted(_)) => {
                latest.remove(node_id.as_str());
            }
            _ => {}
        }
    }
    latest
}

/// The event schema's snake_case task-status names — user output never
/// leaks Rust identifiers.
pub(crate) fn task_status_label(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Ready => "ready",
        TaskStatus::Running => "running",
        TaskStatus::Done => "done",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Failed => "failed",
    }
}
