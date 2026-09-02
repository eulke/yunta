//! `yunta gc`: removes `run.dir`/worktree pairs for terminal runs once
//! `storage.retention_days` has passed since their last event.
//!
//! A finished run's `run.dir` can be deleted without losing anything:
//! `events.jsonl` already exported a self-contained copy at close, and
//! worktree isolation deliberately leaves the git worktree on disk "for
//! inspection" after a run finishes (`worktree.rs`'s own doc comment)
//! rather than removing it — `gc` is the mechanism that eventually
//! reclaims that disk, not `release_worktree` (a no-op for
//! `Isolation::Worktree` by design).
//!
//! Database retention: the event-log rows have their own deadline, with
//! one explicit death order — run.dir (whose exported `events.jsonl` is
//! the self-contained copy) dies first, rows die on a *later* gc pass,
//! only for a run whose run.dir is already gone. The database is never
//! the first copy of a run to die.

use std::process::ExitCode;

use yunta_core::RunId;
use yunta_storage::Storage;

use crate::project;

pub fn gc(dry_run: bool) -> ExitCode {
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

    let Some(retention_days) = project
        .config
        .storage
        .as_ref()
        .and_then(|s| s.retention_days)
    else {
        println!(
            "`storage.retention_days` isn't configured — nothing to reclaim until it is, \
             since `gc` has no default retention to guess"
        );
        return ExitCode::SUCCESS;
    };

    let storage = match Storage::open(&project.storage_path) {
        Ok(storage) => storage,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let run_ids = match storage
        .list_runs()
        .map(|runs| runs.into_iter().map(|run| run.run_id).collect::<Vec<_>>())
    {
        Ok(ids) => ids,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let now = yunta_core::Clock::now(&yunta_core::SystemClock);
    let mut reclaimed = 0usize;
    for run_id in run_ids {
        let events = match storage.events_for_run(&run_id) {
            Ok(events) => events,
            Err(e) => {
                eprintln!("warning: run `{run_id}`: {e}");
                continue;
            }
        };
        let Some(last) = events.last() else { continue };

        let state = yunta_engine::derive(&events);
        let is_terminal = state.broken.is_some()
            || events.iter().any(|e| {
                matches!(
                    e.payload(),
                    Some(yunta_core::events::EventPayload::RunFinished(_))
                )
            });
        if !is_terminal {
            continue;
        }

        let age_days = (now - last.timestamp).num_days();
        if age_days < retention_days as i64 {
            continue;
        }

        // Death order: files first, rows on a later pass. A run whose
        // run.dir is already gone (a previous gc, or a human) has its
        // rows purged now; one whose files still exist loses only the
        // files this pass.
        let run_dir = project.runs_root.join(run_id.as_str());
        if run_dir.exists() {
            if remove_run(&project, &run_id, dry_run) {
                reclaimed += 1;
            }
        } else if dry_run {
            println!("would purge {} event(s) for run {run_id}", events.len());
            reclaimed += 1;
        } else {
            match storage.purge_run(&run_id) {
                Ok(purged) => {
                    println!("purged {} event(s) for run {run_id}", purged.rows);
                    reclaimed += 1;
                }
                Err(e) => eprintln!("warning: run `{run_id}`: {e}"),
            }
        }
    }

    if reclaimed == 0 {
        println!("nothing to reclaim");
    } else if dry_run {
        println!("{reclaimed} run(s) would be reclaimed");
    } else {
        println!("{reclaimed} run(s) reclaimed");
    }
    ExitCode::SUCCESS
}

fn remove_run(project: &project::Project, run_id: &RunId, dry_run: bool) -> bool {
    let run_dir = project.runs_root.join(run_id.as_str());
    let worktree_dir = project.worktrees_root.join(run_id.as_str());
    let mut touched = false;

    for dir in [&run_dir, &worktree_dir] {
        if !dir.exists() {
            continue;
        }
        touched = true;
        if dry_run {
            println!("would remove {}", dir.display());
        } else if let Err(e) = std::fs::remove_dir_all(dir) {
            eprintln!("warning: failed to remove {}: {e}", dir.display());
        } else {
            println!("removed {}", dir.display());
        }
    }
    touched
}
