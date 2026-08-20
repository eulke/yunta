//! `yunta gc` (T7.1, §2.2/§8.3): removes `run.dir`/worktree pairs for
//! terminal runs once `storage.retention_days` has passed since their
//! last event.
//!
//! §8.3 is explicit that a finished run's `run.dir` can be deleted
//! without losing anything: `events.jsonl` already exported a
//! self-contained copy at close, and worktree isolation deliberately
//! leaves the git worktree on disk "for inspection" after a run
//! finishes (`worktree.rs`'s own doc comment) rather than removing it —
//! `gc` is the mechanism that eventually reclaims that disk, not
//! `release_worktree` (a no-op for `Isolation::Worktree` by design).
//!
//! **Scope, named rather than silently narrower than §8.3's own text:**
//! this command only ever touches the filesystem (`run.dir` and its
//! worktree). §8.3 also says the base event log "se conserva según
//! `storage.retention_days`" — implying the *database* rows have their
//! own retention story — but `yunta-storage` exposes no
//! delete-events-older-than-X call today, and inventing one as a side
//! effect of a CLI command would be exactly the kind of debt CLAUDE.md
//! asks not to smuggle in. The database's own retention consumer is
//! still open; see `docs/m0-status.md`.

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
    let run_ids = match storage.list_run_ids() {
        Ok(ids) => ids,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let now = chrono::Utc::now();
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
            || events
                .iter()
                .any(|e| matches!(e.payload, yunta_core::events::EventPayload::RunFinished(_)));
        if !is_terminal {
            continue;
        }

        let age_days = (now - last.timestamp).num_days();
        if age_days < retention_days as i64 {
            continue;
        }

        if remove_run(&project, &run_id, dry_run) {
            reclaimed += 1;
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
