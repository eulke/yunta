//! `yunta status <run_id>`: progress derived from the event log alone,
//! never an estimate or an agent's own report. Two levels — **flow**
//! (nodes finished over the DAG frozen in the manifest) and **task**
//! (ledger tasks done/total) — presented as counters with context,
//! never a percentage: a percentage lies the moment a reroute grows
//! the denominator.

use yunta_core::events::{EventPayload, Failure, StoredEvent, TaskStatus};
use yunta_core::{ArtifactFailure, ArtifactKind, Diagnostic, FileProblem};
use yunta_core::{Manifest, ModeName, NodeId, RunId};
use yunta_engine::NodeState;

use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::load_yaml;

/// Every node id the manifest's frozen DAG declares, `parallel` children
/// included — the fixed denominator the flow counter measures against.
/// Mirrors `yunta_engine::check`'s own id collection (never
/// exported, so duplicated here rather than widened just for this).
/// The ids the run's mode includes (`None` = no narrowing) — the same
/// derivation the scheduler itself uses (`yunta_engine::mode_included_nodes`),
/// so status can never disagree with what actually ran.
fn mode_included_ids(
    workflow: &yunta_core::Workflow,
    mode: &ModeName,
) -> Option<std::collections::HashSet<NodeId>> {
    yunta_engine::mode_included_nodes(workflow, mode)
}

/// Counters with context, never percentages — derived from exactly
/// `events` and the run's own frozen `manifest`. Shared by
/// `yunta status` (one run, in detail) and `yunta list --runs`
/// (every local run, one line each) so the two surfaces can never
/// disagree about what a run's progress means.
pub(crate) fn progress_summary(events: &[StoredEvent], manifest: &Manifest) -> String {
    let declared_nodes: Vec<NodeId> = manifest
        .workflow
        .iter_nodes()
        .map(|node| node.id.clone())
        .collect();

    let state = yunta_engine::derive(events);

    let reroutes = events
        .iter()
        .filter(|e| matches!(e.payload(), Some(EventPayload::NodeRerouted(_))))
        .count();

    let phase = if let Some(diagnostic) = &state.broken {
        format!("broken — {diagnostic}")
    } else {
        match events.iter().rev().find_map(|e| match e.payload() {
            Some(EventPayload::RunFinished(_)) => Some("finished".to_string()),
            // A paused run is waiting on a person, not stuck — e.g.
            // "waiting on gate approve-plan" is what a `run_paused`
            // reason already reads like, so this reuses it verbatim
            // rather than inventing a second vocabulary for the same
            // fact.
            Some(EventPayload::RunPaused(p)) => Some(format!(
                "waiting — {}",
                yunta_core::text::one_line(&p.reason)
            )),
            Some(EventPayload::RunResumed(_) | EventPayload::NodeStarted(_)) => {
                Some("running".to_string())
            }
            _ => None,
        }) {
            Some(phase) => phase,
            None => "created".to_string(),
        }
    };

    let nodes_terminated = state
        .nodes
        .values()
        .filter(|n| matches!(n, NodeState::Finished { .. } | NodeState::Failed { .. }))
        .count();
    let waiting = state
        .nodes
        .values()
        .filter(|n| matches!(n, NodeState::Waiting { .. }))
        .count();
    // The run's mode narrows the denominator — a change that must be
    // visible and attributable, never silent. The mode comes from
    // `run_created`, frozen there at creation; the excluded nodes
    // render as `skipped`, not omitted.
    let mode = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(EventPayload::RunCreated(p)) => Some(p.mode.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let included = mode_included_ids(&manifest.workflow, &mode);
    let skipped = match &included {
        Some(included) => declared_nodes
            .iter()
            .filter(|id| !included.contains(*id))
            .count(),
        None => 0,
    };
    let denominator = declared_nodes.len() - skipped;
    let mut summary = format!("{nodes_terminated}/{denominator} nodes");
    if skipped > 0 {
        summary.push_str(&format!(" · {skipped} skipped (mode: {mode})"));
    }
    if waiting > 0 {
        summary.push_str(&format!(" · {waiting} waiting"));
    }
    if !state.tasks.is_empty() {
        let tasks_done = state
            .tasks
            .values()
            .filter(|s| matches!(s, TaskStatus::Done))
            .count();
        summary = format!("{tasks_done}/{} tasks · {summary}", state.tasks.len());
    }
    summary.push_str(&format!(" · {reroutes} reroutes · {phase}"));
    if let Some(note) = super::unknown_kinds_note(&yunta_engine::unknown_kind_counts(&state)) {
        summary.push_str(&format!(" · {note}"));
    }
    summary
}

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

    if json {
        return crate::json::print_json(&status_json(run_id, &events, &manifest));
    }

    println!("run {run_id}: {}", progress_summary(&events, &manifest));

    let state = yunta_engine::derive(&events);
    if !state.nodes.is_empty() {
        println!("nodes:");
        let mut nodes: Vec<_> = state.nodes.iter().collect();
        nodes.sort_by(|a, b| a.0.cmp(b.0));
        for (id, node) in nodes {
            println!("  {id}: {}", node_label(node));
        }
    }

    if !state.tasks.is_empty() {
        println!("tasks:");
        let mut tasks: Vec<_> = state.tasks.iter().collect();
        tasks.sort_by(|a, b| a.0.cmp(b.0));
        for (id, status) in tasks {
            println!("  {id}: {}", task_status_label(status));
        }
    }

    print_failures(&state);

    println!(
        "tokens: {} in / {} out",
        state.total_tokens.input, state.total_tokens.output
    );
    Ok(Outcome::Success)
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
    for (id, failure) in failed {
        println!("  {id}:");
        match failure {
            Failure::Artifacts { artifacts } => {
                for artifact in artifacts {
                    println!(
                        "{}",
                        yunta_core::text::indent(&artifact.to_string(), "    ")
                    );
                }
            }
            Failure::Message { outcome } => {
                println!("{}", yunta_core::text::indent(outcome, "    "));
            }
        }
    }
}

/// One display label for a node's derived state — the same text
/// `yunta status` prints and the `status_json` DTO carries, so the two
/// never drift.
fn node_label(node: &NodeState) -> String {
    match node {
        NodeState::Running { attempt } => format!("running (attempt {attempt})"),
        NodeState::Finished { outcome, .. } => {
            format!("finished — {}", yunta_core::text::one_line(outcome))
        }
        NodeState::Failed { failure, .. } => {
            format!(
                "failed — {}",
                yunta_core::text::one_line(&failure.to_string())
            )
        }
        NodeState::Waiting { external_ref } => match external_ref {
            Some(external_ref) => format!("waiting — {external_ref}"),
            None => "waiting".to_string(),
        },
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

/// Derives a run's [`StatusJson`] from its event log and frozen manifest
/// — the print-free core `status --json` and the control plane share, so
/// a program reads the same document whichever surface it asks.
pub(crate) fn status_json(
    run_id: &RunId,
    events: &[StoredEvent],
    manifest: &Manifest,
) -> StatusJson {
    let state = yunta_engine::derive(events);
    StatusJson {
        schema_version: crate::json::SCHEMA_VERSION,
        run_id: run_id.to_string(),
        summary: progress_summary(events, manifest),
        nodes: state
            .nodes
            .iter()
            .map(|(id, node)| (id.to_string(), node_label(node)))
            .collect(),
        tasks: state
            .tasks
            .iter()
            .map(|(id, status)| (id.to_string(), task_status_label(status)))
            .collect(),
        diagnostics: node_diagnostics(events),
        tokens: TokensJson {
            input: state.total_tokens.input,
            output: state.total_tokens.output,
        },
    }
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
