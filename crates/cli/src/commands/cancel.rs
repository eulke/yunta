//! `yunta cancel <run_id>` (T7.1, Spec Adapter §2/A4: "interrupt → kill,
//! extermina el árbol de procesos completo").
//!
//! **Known gap, named rather than emulated (A6's own principle applied
//! to the CLI itself):** every spawned session, hook and executor
//! process is deliberately placed in its own OS process group so the
//! *in-process* interrupt→kill path (budget/timeout/`join: any`) can
//! extinguish it without taking the `yunta` binary down too — but that
//! also means nothing today lets a *separate* `yunta cancel` invocation
//! find and signal those processes: no pidfile, no daemon, no socket.
//! `yunta run --detach` (M8, D101) is the first thing that will actually
//! need this and is the natural trigger to build it. Until then this
//! command only ever reports what the log already says — it refuses to
//! claim a cancellation it can't perform.

use std::process::ExitCode;

use yunta_core::RunId;
use yunta_engine::NodeState;
use yunta_storage::Storage;

use crate::project;

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
            yunta_core::events::EventPayload::RunFinished(_)
                | yunta_core::events::EventPayload::RunPaused(_)
        )
    });

    if state.broken.is_some() || has_terminal_run_event {
        println!("run {run_id}: already stopped — nothing to cancel");
        return ExitCode::SUCCESS;
    }

    let has_live_node = state
        .nodes
        .values()
        .any(|n| matches!(n, NodeState::Running { .. }));
    if !has_live_node {
        println!("run {run_id}: no node in progress — nothing to cancel");
        return ExitCode::SUCCESS;
    }

    eprintln!(
        "error: run `{run_id}` has a node in progress, but this build has no channel to \
         signal a separate live `yunta run` process — see this command's own module doc for \
         why. `yunta resume {run_id}` recovers the run once its `yunta run` process has \
         stopped (crash, Ctrl-C or otherwise — §8.1 treats them the same), picking up the \
         orphaned node per its own `on_interrupt` policy; it does not itself stop anything \
         still running."
    );
    ExitCode::FAILURE
}
