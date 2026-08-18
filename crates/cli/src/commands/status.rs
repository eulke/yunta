//! `yunta status <run_id>` (T7.1 parcial, §8.5): two-level progress —
//! nodes and tasks — derived from the event log alone.

use std::process::ExitCode;

use yunta_core::events::{EventPayload, TaskStatus};
use yunta_core::RunId;
use yunta_engine::NodeState;
use yunta_storage::Storage;

use crate::project;

pub fn status(run_id: &str) -> ExitCode {
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

    let run_id = RunId::from(run_id);
    let events = match storage.events_for_run(&run_id) {
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

    let state = yunta_engine::derive(&events);

    let phase = if let Some(diagnostic) = &state.broken {
        format!("broken — {diagnostic}")
    } else {
        match events.iter().rev().find_map(|e| match &e.payload {
            EventPayload::RunFinished(_) => Some("finished".to_string()),
            EventPayload::RunPaused(p) => Some(format!("paused — {}", p.reason)),
            EventPayload::RunResumed(_) | EventPayload::NodeStarted(_) => {
                Some("in progress".to_string())
            }
            _ => None,
        }) {
            Some(phase) => phase,
            None => "created".to_string(),
        }
    };
    println!("run {run_id}: {phase}");

    if !state.nodes.is_empty() {
        println!("nodes:");
        let mut nodes: Vec<_> = state.nodes.iter().collect();
        nodes.sort_by(|a, b| a.0.cmp(b.0));
        for (id, node) in nodes {
            let line = match node {
                NodeState::Running { attempt } => format!("running (attempt {attempt})"),
                NodeState::Finished { outcome, .. } => format!("finished — {outcome}"),
                NodeState::Failed { outcome, .. } => format!("failed — {outcome}"),
            };
            println!("  {id}: {line}");
        }
    }

    if !state.tasks.is_empty() {
        let done = state
            .tasks
            .values()
            .filter(|s| matches!(s, TaskStatus::Done))
            .count();
        println!("tasks: {done}/{} done", state.tasks.len());
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
