//! `yunta status <run_id>`: progress derived from the event log alone,
//! never an estimate or an agent's own report. Two levels — **flow**
//! (nodes finished over the DAG frozen in the manifest) and **task**
//! (ledger tasks done/total) — presented as counters with context,
//! never a percentage: a percentage lies the moment a reroute grows
//! the denominator.

use std::process::ExitCode;

use yunta_core::events::{EventPayload, StoredEvent, TaskStatus};
use yunta_core::{Manifest, ModeName, NodeId, RunId};
use yunta_engine::NodeState;
use yunta_storage::Storage;

use crate::load_yaml;
use crate::project;

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
            Some(EventPayload::RunPaused(p)) => Some(format!("waiting — {}", p.reason)),
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
    let unknown = yunta_engine::unknown_kind_counts(&state);
    if !unknown.is_empty() {
        let kinds: Vec<String> = unknown
            .iter()
            .map(|count| format!("{} ×{}", count.kind, count.events))
            .collect();
        summary.push_str(&format!(
            " · unknown event kind(s), interpreted partially: {}",
            kinds.join(", ")
        ));
    }
    summary
}

pub fn status(run_id: &RunId) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let project = match project::resolve(&cwd) {
        Ok(project) => project,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let storage = match Storage::open(&project.storage_path) {
        Ok(storage) => storage,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let events = match storage.events_for_run(run_id) {
        Ok(events) => events,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if events.is_empty() {
        eprintln!(
            "error: no run `{run_id}` in {}",
            project.storage_path.display()
        );
        return ExitCode::FAILURE;
    }

    // Search order (current runs root, then the default) — the run's
    // own frozen paths take over once the manifest is open.
    let manifest_path = crate::project::find_run_dir(&project, run_id.as_str())
        .unwrap_or_else(|| project.runs_root.join(run_id.as_str()))
        .join("manifest.yaml");
    let manifest: Manifest = match load_yaml(&manifest_path, "run manifest") {
        Ok(manifest) => manifest,
        Err(code) => return code,
    };

    println!("run {run_id}: {}", progress_summary(&events, &manifest));

    let state = yunta_engine::derive(&events);
    if !state.nodes.is_empty() {
        println!("nodes:");
        let mut nodes: Vec<_> = state.nodes.iter().collect();
        nodes.sort_by(|a, b| a.0.cmp(b.0));
        for (id, node) in nodes {
            let line = match node {
                NodeState::Running { attempt } => format!("running (attempt {attempt})"),
                NodeState::Finished { outcome, .. } => format!("finished — {outcome}"),
                NodeState::Failed { outcome, .. } => format!("failed — {outcome}"),
                NodeState::Waiting { external_ref } => match external_ref {
                    Some(external_ref) => format!("waiting — {external_ref}"),
                    None => "waiting".to_string(),
                },
            };
            println!("  {id}: {line}");
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

    println!(
        "tokens: {} in / {} out",
        state.total_tokens.input, state.total_tokens.output
    );
    ExitCode::SUCCESS
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
