//! `yunta cancel <run_id>` (T7.1/DI-08, Spec Adapter §2/A4: "interrupt →
//! kill, extermina el árbol de procesos completo").
//!
//! The channel is `run.dir/scratch/engine.json` (DI-08): the live
//! engine's pid plus the process-group ids of everything it spawned.
//! Three cases, each explicit:
//!
//! 1. **Engine alive** — SIGINT to the engine pid; its own Ctrl-C path
//!    (interrupt→kill, T3.3/T4.6) exterminates the tree and records
//!    `run_paused { reason: "cancelled by user" }`. This command waits
//!    for that terminal in the log, escalating to SIGKILL on the
//!    registered process groups if it doesn't arrive in time.
//! 2. **Engine dead, pgids registered** (a crash) — kill the orphaned
//!    groups directly, record `run_paused { reason: "cancelled after
//!    crash" }`, delete the leftover registry.
//! 3. **No registry** — nothing to signal; report what the log says.

use std::process::ExitCode;
use std::time::Duration;

use yunta_core::events::EventPayload;
use yunta_core::RunId;
use yunta_engine::NodeState;
use yunta_storage::Storage;

use crate::project;

/// How long the engine gets to react to the SIGINT before the escalation
/// — generous next to the engine's own 200ms interrupt grace, because a
/// mid-batch engine finishes killing its sessions before it pauses.
const ENGINE_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn cancel(run_id: &str) -> ExitCode {
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
    let has_terminal_run_event = events.iter().any(|e| {
        matches!(
            e.payload,
            EventPayload::RunFinished(_) | EventPayload::RunPaused(_)
        )
    });

    if state.broken.is_some() || has_terminal_run_event {
        println!("run {run_id}: already stopped — nothing to cancel");
        return ExitCode::SUCCESS;
    }

    let run_dir = project.runs_root.join(run_id.as_str());
    let Some(registry) = yunta_engine::read_registry(&run_dir) else {
        // Case 3 — no channel. Pre-DI-08 behavior, now the exception
        // rather than the rule.
        let has_live_node = state
            .nodes
            .values()
            .any(|n| matches!(n, NodeState::Running { .. }));
        if !has_live_node {
            println!("run {run_id}: no node in progress — nothing to cancel");
            return ExitCode::SUCCESS;
        }
        eprintln!(
            "error: run `{run_id}` has a node in progress but no `engine.json` to signal \
             through — the engine that ran it predates this build, or its scratch directory \
             is gone. `yunta resume {run_id}` recovers the run once its process has stopped."
        );
        return ExitCode::FAILURE;
    };

    if yunta_engine::process_alive(registry.engine_pid) {
        // Case 1 — the engine handles the rest itself.
        println!(
            "run {run_id}: signalling the live engine (pid {})",
            registry.engine_pid
        );
        let _ = std::process::Command::new("kill")
            .args(["-INT", &registry.engine_pid.to_string()])
            .status();

        let deadline = tokio::time::Instant::now() + ENGINE_SHUTDOWN_TIMEOUT;
        loop {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let events = match storage.events_for_run(&run_id) {
                Ok(events) => events,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let terminal = events.iter().any(|e| {
                matches!(
                    e.payload,
                    EventPayload::RunFinished(_) | EventPayload::RunPaused(_)
                )
            });
            if terminal {
                println!("run {run_id}: cancelled — the log has its terminal");
                return ExitCode::SUCCESS;
            }
            if tokio::time::Instant::now() >= deadline {
                eprintln!(
                    "run {run_id}: the engine did not stop within {ENGINE_SHUTDOWN_TIMEOUT:?} \
                     — escalating to SIGKILL on its process groups (A4)"
                );
                for pgid in &registry.process_groups {
                    kill_group(*pgid);
                }
                let _ = std::process::Command::new("kill")
                    .args(["-KILL", &registry.engine_pid.to_string()])
                    .status();
                return ExitCode::FAILURE;
            }
        }
    }

    // Case 2 — the engine crashed; its leftovers are ours to clean.
    for pgid in &registry.process_groups {
        kill_group(*pgid);
    }
    let paused = storage.append_event(&yunta_core::events::Event {
        run_id: run_id.clone(),
        seq: 0,
        timestamp: chrono::Utc::now(),
        node_id: None,
        payload: EventPayload::RunPaused(yunta_core::events::RunPausedPayload {
            reason: "cancelled after crash".to_string(),
        }),
    });
    if let Err(e) = paused {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }
    if let Err(e) = std::fs::remove_file(yunta_engine::registry_path(&run_dir)) {
        if e.kind() != std::io::ErrorKind::NotFound {
            eprintln!("warning: could not delete engine.json: {e}");
        }
    }
    println!(
        "run {run_id}: engine (pid {}) was already dead — killed {} orphaned process \
         group(s), recorded the pause",
        registry.engine_pid,
        registry.process_groups.len()
    );
    ExitCode::SUCCESS
}

/// SIGKILL to a whole process group — the `--` before the negative pid
/// is load-bearing (procps-ng parses `-KILL -123` as two flags without
/// it). A group already gone is the desired end state, not an error.
fn kill_group(pgid: u32) {
    let _ = std::process::Command::new("kill")
        .args(["-KILL", "--", &format!("-{pgid}")])
        .status();
}
