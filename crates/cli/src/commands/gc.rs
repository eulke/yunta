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
//! A run is found and reclaimed by the paths *frozen in its manifest*,
//! not the current config's: `run.dir` through the same search rule
//! `status`/`resume` use, and the worktree through the root the manifest
//! froze — so a `paths.*` change after the run was created never orphans
//! either. A removal that fails is warned and left uncounted; only a run
//! whose whole footprint was actually reclaimed is reported as such.
//!
//! Database retention: the event-log rows have their own deadline, with
//! one explicit death order — run.dir (whose exported `events.jsonl` is
//! the self-contained copy) dies first, rows die on a *later* gc pass,
//! only for a run whose run.dir is already gone. The database is never
//! the first copy of a run to die.

use std::path::{Path, PathBuf};

use yunta_core::{Isolation, Manifest, RunId};

use crate::context::Context;
use crate::error::{warn, CliError, Outcome};
use crate::project::Project;
use yunta_core::events::RunEvent;

pub async fn gc(dry_run: bool) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let removed = reclaim(&ctx, dry_run)?;
    for run_id in &removed {
        forget_units(&ctx, run_id).await;
    }
    Ok(Outcome::Success)
}

/// What git still holds of a run whose files are gone: the checkouts its
/// units worked in, and the branches they worked on — a blocked task's
/// left work among them, which nothing reads once the run is collected.
async fn forget_units(ctx: &Context, run_id: &RunId) {
    let supervision = ctx.supervision();
    if let Err(e) = yunta_engine::git::output(&ctx.cwd, &["worktree", "prune"], supervision).await {
        warn(format!("run {}: {e}", run_id.handle()));
        return;
    }
    let ours = format!("refs/heads/{}/", yunta_engine::run_units(run_id));
    let branches = match yunta_engine::git::output(
        &ctx.cwd,
        &["for-each-ref", "--format=%(refname:short)", &ours],
        supervision,
    )
    .await
    {
        Ok(branches) => branches,
        Err(e) => return warn(format!("run {}: {e}", run_id.handle())),
    };
    for branch in branches.lines().map(str::trim).filter(|b| !b.is_empty()) {
        if let Err(e) =
            yunta_engine::git::output(&ctx.cwd, &["branch", "-D", branch], supervision).await
        {
            warn(format!("run {}: {e}", run_id.handle()));
        }
    }
}

/// Removes every terminal run past retention — its files on this pass,
/// its rows on a later one — and answers the runs whose files went.
fn reclaim(ctx: &Context, dry_run: bool) -> Result<Vec<RunId>, CliError> {
    let Some(retention_days) = ctx
        .project
        .config
        .storage
        .as_ref()
        .and_then(|s| s.retention_days)
    else {
        println!(
            "`storage.retention_days` isn't configured — nothing to reclaim until it is, \
             since `gc` has no default retention to guess"
        );
        return Ok(Vec::new());
    };

    let storage = ctx.storage()?;
    let run_ids = storage
        .list_runs()
        .map(|runs| runs.into_iter().map(|run| run.run_id).collect::<Vec<_>>())?;

    let now = yunta_core::Clock::now(&ctx.clock);
    let mut reclaimed = 0usize;
    let mut removed = Vec::new();
    for run_id in run_ids {
        let events = match storage.events_for_run(&run_id) {
            Ok(events) => events,
            Err(e) => {
                warn(format!("run {}: {e}", run_id.handle()));
                continue;
            }
        };
        let Some(last) = events.last() else { continue };

        let state = yunta_engine::derive(&events);
        let is_terminal = state.broken.is_some()
            || events.iter().any(|e| {
                matches!(
                    e.payload(),
                    Some(yunta_core::events::EventPayload::Run(RunEvent::Finished(_)))
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
        // run.dir is still on disk (found by its frozen paths) loses only
        // the files this pass; one whose run.dir is already gone (a
        // previous gc, or a human) has its rows purged now.
        match ctx.project.run_dir(run_id.as_str()) {
            Some(run_dir) => {
                if remove_run(ctx, &run_dir, &run_id, dry_run) {
                    reclaimed += 1;
                    if !dry_run {
                        removed.push(run_id.clone());
                    }
                }
            }
            None if dry_run => {
                println!(
                    "would purge {} of run {}",
                    yunta_core::text::counted(events.len(), "event"),
                    run_id.handle()
                );
                reclaimed += 1;
            }
            None => match storage.purge_run(&run_id) {
                Ok(purged) => {
                    println!(
                        "purged {} of run {}",
                        yunta_core::text::counted(purged.rows, "event"),
                        run_id.handle()
                    );
                    reclaimed += 1;
                }
                Err(e) => warn(format!("run {}: {e}", run_id.handle())),
            },
        }
    }

    if reclaimed == 0 {
        println!("nothing to reclaim");
    } else if dry_run {
        println!(
            "{} would be reclaimed",
            yunta_core::text::counted(reclaimed, "run")
        );
    } else {
        println!("{} reclaimed", yunta_core::text::counted(reclaimed, "run"));
    }
    Ok(removed)
}

/// Removes a terminal run's on-disk footprint — its `run.dir` and, for a
/// worktree-isolated run, the worktree frozen in its manifest — and
/// reports whether every directory that was present got removed. A run
/// with a removal that failed is warned about and returns `false`, so it
/// is never counted as reclaimed while some of its disk survives. Under
/// `dry_run` nothing is removed and every present directory counts as if
/// it had been.
fn remove_run(ctx: &Context, run_dir: &Path, run_id: &RunId, dry_run: bool) -> bool {
    let worktree = worktree_of(&ctx.project, run_dir, run_id);
    let shown = |dir: &Path| crate::render::paths::shown(dir, &ctx.cwd, ctx.env.home.as_deref());
    let mut removed_any = false;
    let mut all_removed = true;

    for dir in [Some(run_dir.to_path_buf()), worktree]
        .into_iter()
        .flatten()
    {
        if !dir.exists() {
            continue;
        }
        if dry_run {
            println!("would remove {}", shown(&dir));
            removed_any = true;
        } else if let Err(e) = std::fs::remove_dir_all(&dir) {
            warn(format!("failed to remove {}: {e}", shown(&dir)));
            all_removed = false;
        } else {
            println!("removed {}", shown(&dir));
            removed_any = true;
        }
    }
    removed_any && all_removed
}

/// The worktree directory `gc` should reclaim for this run, read from
/// the root frozen in its manifest — or `None` when there is none to
/// reclaim: an `isolation: none` run works on the checkout itself, never
/// a dedicated worktree, so gc removes nothing for it beyond `run.dir`. A
/// manifest that cannot be read is surfaced and treated as no worktree —
/// `run.dir` is still reclaimed, its worktree (if any) left for a human,
/// never guessed at from the current config.
fn worktree_of(project: &Project, run_dir: &Path, run_id: &RunId) -> Option<PathBuf> {
    let manifest: Manifest =
        match crate::load_manifest(&yunta_engine::run_dir::manifest_path(run_dir))
            .map(|manifest| manifest.doc)
        {
            Ok(manifest) => manifest,
            Err(e) => {
                warn(format!("run {}: {e}", run_id.handle()));
                return None;
            }
        };
    match manifest.isolation {
        Isolation::Worktree => Some(project.worktrees_root_for(&manifest).join(run_id.as_str())),
        Isolation::None => None,
    }
}
