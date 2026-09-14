//! `yunta cancel <run_id>`: interrupt, then kill — exterminates the
//! run's whole process tree rather than leaving anything orphaned.
//!
//! The channel is `run.dir/scratch/engine.json`: the live engine's pid
//! plus the process-group ids of everything it spawned. Three cases,
//! each explicit:
//!
//! 1. **Engine alive** — SIGINT to the engine pid; its own Ctrl-C path
//!    (interrupt→kill) exterminates the tree and records
//!    `run_paused { reason: "cancelled by user" }`. This command waits
//!    for that terminal in the log, escalating to SIGKILL on the
//!    registered process groups if it doesn't arrive in time.
//! 2. **Engine dead, pgids registered** (a crash) — kill the orphaned
//!    groups directly, record `run_paused { reason: "cancelled after
//!    crash" }`, delete the leftover registry.
//! 3. **No registry** — nothing to signal; report what the log says.

use std::time::Duration;

use yunta_core::process::signal::{signal_group, signal_process, Liveness, Signal};
use yunta_core::{describe, events::EventPayload, Pid, RunId};
use yunta_engine::NodeState;

use crate::commands::advice;
use crate::context::Context;
use crate::error::{note, warn, CliError, Outcome};
use yunta_core::events::RunEvent;

/// How long the engine gets to react to the SIGINT before the escalation
/// — generous next to the engine's own 200ms interrupt grace, because a
/// mid-batch engine finishes killing its sessions before it pauses.
const ENGINE_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(15);

/// How often `cancel` re-reads the log while it waits for the engine to
/// record its terminal after the SIGINT.
const ENGINE_SHUTDOWN_POLL: Duration = Duration::from_millis(200);

pub async fn cancel(run_id: &RunId) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let storage = ctx.async_storage().await?;
    let events = storage.events_for_run(run_id.clone()).await?;
    if events.is_empty() {
        return Err(CliError::msg(format!(
            "no run `{run_id}` in {}",
            ctx.project.storage_path.display()
        )));
    }

    let state = yunta_engine::derive(&events);
    let has_terminal_run_event = events.iter().any(|e| {
        matches!(
            e.payload(),
            Some(EventPayload::Run(RunEvent::Finished(_)) | EventPayload::Run(RunEvent::Paused(_)))
        )
    });

    if state.broken.is_some() || has_terminal_run_event {
        println!("run {run_id}: already stopped — nothing to cancel");
        return Ok(Outcome::Success);
    }

    let run_dir = ctx
        .project
        .run_dir(run_id.as_str())
        .unwrap_or_else(|| ctx.project.runs_root.join(run_id.as_str()));
    let registry = match yunta_engine::read_registry(&run_dir) {
        yunta_engine::Registry::Read(registry) => registry.doc,
        // A registry that will not read is not a registry that was
        // never written: the person is told which of the two it is,
        // because only one of them is a broken file on their disk.
        yunta_engine::Registry::Corrupt(error) => {
            return Err(CliError::msg(format!(
                "run `{run_id}`: its `engine.json` is there and this binary cannot read it \
                 ({}) — `{}` recovers the run once its process has stopped.",
                describe(&error),
                advice::resume(run_id)
            )))
        }
        // Case 3 — no channel. Legacy fallback behavior, now the
        // exception rather than the rule.
        yunta_engine::Registry::Absent => {
            let has_live_node = state
                .nodes
                .values()
                .any(|record| matches!(record.state, Some(NodeState::Running { .. })));
            if !has_live_node {
                println!("run {run_id}: no node in progress — nothing to cancel");
                return Ok(Outcome::Success);
            }
            return Err(CliError::msg(format!(
                "run `{run_id}` has a node in progress but no `engine.json` to signal \
                 through — the engine that ran it predates this build, or its scratch \
                 directory is gone. `{}` recovers the run once its process has stopped.",
                advice::resume(run_id)
            )));
        }
    };

    if engine_is_alive(&registry) == Liveness::Alive {
        // Case 1 — the engine handles the rest itself.
        println!(
            "run {run_id}: signalling the live engine (pid {})",
            registry.engine_pid
        );
        signal_process(registry.engine_pid, Signal::SIGINT).map_err(|e| {
            CliError::msg(format!(
                "{} — retry `{}`",
                describe(&e),
                advice::cancel(run_id)
            ))
        })?;

        let deadline = tokio::time::Instant::now() + ENGINE_SHUTDOWN_TIMEOUT;
        loop {
            tokio::time::sleep(ENGINE_SHUTDOWN_POLL).await;
            let events = storage.events_for_run(run_id.clone()).await?;
            let terminal = events.iter().any(|e| {
                matches!(
                    e.payload(),
                    Some(
                        EventPayload::Run(RunEvent::Finished(_))
                            | EventPayload::Run(RunEvent::Paused(_))
                    )
                )
            });
            if terminal {
                println!("run {run_id}: cancelled — the log has its terminal");
                return Ok(Outcome::Success);
            }
            if tokio::time::Instant::now() >= deadline {
                note(format!(
                    "run {run_id}: the engine did not stop within {ENGINE_SHUTDOWN_TIMEOUT:?} \
                     — escalating to SIGKILL on its process groups"
                ));
                kill_groups(&registry.process_groups);
                if let Err(e) = signal_process(registry.engine_pid, Signal::SIGKILL) {
                    if !e.is_gone() {
                        warn(describe(&e));
                    }
                }
                return Ok(Outcome::Reported);
            }
        }
    }

    // Case 2 — the engine crashed; its leftovers are ours to clean. The
    // `run_paused` is emitted through the engine, not hand-built here, so
    // the CLI never stamps an event with a clock of its own.
    kill_groups(&registry.process_groups);
    yunta_engine::record_pause_after_crash(
        &storage,
        run_id,
        &yunta_core::events::PauseReason::CancelledAfterCrash,
        &ctx.clock,
    )
    .await?;
    if let Err(e) = std::fs::remove_file(yunta_engine::registry_path(&run_dir)) {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn(format!("could not delete engine.json: {e}"));
        }
    }
    println!(
        "run {run_id}: engine (pid {}) was already dead — killed {}, recorded the pause",
        registry.engine_pid,
        yunta_core::text::counted(registry.process_groups.len(), "orphaned process group")
    );
    Ok(Outcome::Success)
}

/// Whether the engine the registry names is still that engine.
///
/// A live pid is not enough. The engine may have died and the host may
/// have handed its number to something else entirely — signalling that
/// would interrupt a process that has nothing to do with this run. The
/// registry wrote down when the engine started, so the answer is the
/// same one the isolation lock asks of its own holder.
fn engine_is_alive(registry: &yunta_engine::EngineProcessFile) -> Liveness {
    yunta_engine::lock::holder_state(
        &yunta_engine::lock::LockOwner {
            schema_version:
                <yunta_engine::lock::LockOwner as yunta_core::persisted::Persisted>::SCHEMA_VERSION,
            pid: registry.engine_pid,
            started_at: registry.started_at,
        },
        &yunta_engine::lock::SystemProbe,
    )
}

/// SIGKILL to every process group the engine registered. A group that is
/// already gone is the end state wanted; any other refusal is printed,
/// never hidden. A pgid that isn't a real run process group (see
/// [`signalable_group`]) is skipped with a loud note rather than signalled.
fn kill_groups(groups: &[Pid]) {
    for pgid in groups {
        if !signalable_group(*pgid) {
            warn(format!(
                "refusing to signal process group {pgid}: not a run's own process group — \
                 engine.json is corrupt, skipping it"
            ));
            continue;
        }
        if let Err(e) = signal_group(*pgid, Signal::SIGKILL) {
            warn(describe(&e));
        }
    }
}

/// Whether `cancel` may signal a process group. `signal_group` is
/// `killpg(pgid)` = `kill(-pgid)`, so `pgid` of 1 becomes `kill(-1)` —
/// every process this user can signal, not one run's tree. The [`Pid`]
/// type already makes 0 and negatives unrepresentable, so this rejects the
/// one dangerous value that survives it: init's group (pgid 1). A registry
/// carrying it is corrupt, and the safe reading is to skip it.
fn signalable_group(pgid: Pid) -> bool {
    pgid.as_u32() > 1
}

#[cfg(test)]
mod tests {
    use super::signalable_group;
    use yunta_core::Pid;

    #[test]
    fn a_run_never_signals_init_or_a_lower_group() {
        // pgid 1 -> killpg(1) -> kill(-1): every process, not one run's
        // tree. `Pid` forbids 0 and negatives, so 1 is the sole dangerous
        // value it still admits — and the one this must reject.
        assert!(!signalable_group(Pid::try_from(1u32).unwrap()));
        assert!(signalable_group(Pid::try_from(2u32).unwrap()));
        assert!(signalable_group(Pid::try_from(4321u32).unwrap()));
    }
}
