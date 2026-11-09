//! `yunta gc`: collects terminal runs once `storage.retention_days` has
//! passed since their last event, and tidies the project's pool.
//!
//! A finished run's `run.dir` can be deleted without losing anything:
//! `events.jsonl` already exported a self-contained copy at close. Its
//! checkout is the project's pool's, so it goes back to the pool with
//! its build rather than being removed; only a checkout a run made before
//! its project kept a pool is taken away. A parked run nobody came back
//! to within retention gives its checkout back the same way, unless a
//! session it left waits to be picked up there.
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
//!
//! What the pass did is one page, said once it is over.

use std::path::{Path, PathBuf};

use surco::look::Look;
use yunta_core::{Isolation, Manifest, RunId};
use yunta_storage::Storage;

use crate::context::Context;
use crate::error::{warn, CliError};
use crate::output::show;
use crate::output::Outcome;
use crate::project::Project;

use yunta_render::page::Collected;

mod pool;
use pool::{adopt, tidy, would_adopt};

pub async fn gc(dry_run: bool, look: &Look) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let limits = ctx
        .project
        .config
        .storage
        .as_ref()
        .and_then(|storage| storage.pool.as_ref());
    let pool = yunta_engine::CheckoutPool::upkeep(
        &ctx.project.worktrees_root,
        &ctx.cwd,
        limits,
        std::sync::Arc::new(ctx.clock),
    );
    let mut collected = Collected::new(dry_run);
    let Reclaimed {
        removed,
        given_back,
        idle,
    } = reclaim(&ctx, &pool, &mut collected).await?;
    for run_id in &removed {
        forget_units(&ctx, &pool, run_id, &mut collected).await;
    }
    if !dry_run {
        for (run_id, tree) in &given_back {
            give_back(&ctx, &pool, run_id, tree, &mut collected).await;
        }
        adopt(&ctx, &pool, &mut collected).await;
        for (run_id, places) in &idle {
            take_old_layouts(&ctx, run_id, places, &mut collected).await;
        }
        tidy(&ctx, &pool, &mut collected).await;
    } else {
        would_adopt(&ctx, &pool, &mut collected).await;
        match pool.report(ctx.supervision()).await {
            Ok(kept) => collected.now = Some(kept.to_string()),
            Err(e) => problem(&mut collected, format!("the project's checkouts: {e}")),
        }
    }
    show(look, &collected);
    Ok(Outcome::Success)
}

/// What a pass collected: the runs whose files went, and the checkouts of
/// the project's pool runs gave back — a collected run's, and a parked
/// run's nobody came back to.
#[derive(Default)]
struct Reclaimed {
    removed: Vec<RunId>,
    given_back: Vec<(RunId, PathBuf)>,
    /// Where each run nothing drives kept checkouts before the project's
    /// pool: its units' directory, and its own tree when outside the pool.
    idle: Vec<(RunId, Vec<PathBuf>)>,
}

/// Gives a run's checkout back to the project's pool, with no branch.
async fn give_back(
    ctx: &Context,
    pool: &yunta_engine::CheckoutPool,
    run_id: &RunId,
    tree: &Path,
    collected: &mut Collected,
) {
    let released = yunta_engine::release_run_checkout(pool, tree, run_id, ctx.supervision());
    if let Err(e) = released.await {
        problem(collected, format!("run {}: {e}", run_id.handle()));
    }
}

/// What git still holds of a run whose files are gone: the checkouts its
/// units worked in, and the branches they worked on — a blocked task's
/// left work among them, which nothing reads once the run is collected.
/// The project's checkouts stay, with no branch, for the runs to come.
async fn forget_units(
    ctx: &Context,
    pool: &yunta_engine::CheckoutPool,
    run_id: &RunId,
    collected: &mut Collected,
) {
    let supervision = ctx.supervision();
    if let Err(e) = yunta_engine::prune_worktrees(&ctx.cwd, supervision).await {
        problem(collected, format!("run {}: {e}", run_id.handle()));
        return;
    }
    if let Err(e) = yunta_engine::forget_run_units(pool, &ctx.cwd, run_id, supervision).await {
        problem(collected, format!("run {}: {e}", run_id.handle()));
    }
}

/// Takes away the clean checkouts run `run_id`, which nothing drives,
/// kept at `places` before the project's pool, and names the ones kept.
async fn take_old_layouts(
    ctx: &Context,
    run_id: &RunId,
    places: &[PathBuf],
    collected: &mut Collected,
) {
    let shown = |dir: &Path| surco::paths::shown(dir, &ctx.cwd, ctx.env.home.as_deref());
    for place in places {
        match yunta_engine::remove_old_checkouts(&ctx.cwd, place, ctx.supervision()).await {
            Ok(done) => {
                for checkout in done {
                    match checkout {
                        yunta_engine::OldCheckout::Removed(path) => collected.did(
                            "removed",
                            format!("{} of run {}", shown(&path), run_id.handle()),
                        ),
                        yunta_engine::OldCheckout::Kept(path) => collected.did(
                            "kept",
                            format!(
                                "{} of run {}: something in it is uncommitted",
                                shown(&path),
                                run_id.handle()
                            ),
                        ),
                    }
                }
            }
            Err(e) => problem(collected, format!("run {}: {e}", run_id.handle())),
        }
    }
}

/// Removes every terminal run past retention — its files on this pass,
/// its rows on a later one — and answers the runs whose files went, with
/// the checkouts of the project's pool to give back: a collected run's,
/// and that of a parked run nobody came back to within retention.
async fn reclaim(
    ctx: &Context,
    pool: &yunta_engine::CheckoutPool,
    collected: &mut Collected,
) -> Result<Reclaimed, CliError> {
    let Some(retention_days) = ctx
        .project
        .config
        .storage
        .as_ref()
        .and_then(|s| s.retention_days)
    else {
        collected.unretained = true;
        return Ok(Reclaimed::default());
    };

    let storage = ctx.storage()?;
    let run_ids = storage
        .list_runs()
        .map(|runs| runs.into_iter().map(|run| run.run_id).collect::<Vec<_>>())?;

    let now = yunta_core::Clock::now(&ctx.clock);
    let mut reclaimed = Reclaimed::default();
    for run_id in run_ids {
        let events = match storage.events_for_run(&run_id) {
            Ok(events) => events,
            Err(e) => {
                problem(collected, format!("run {}: {e}", run_id.handle()));
                continue;
            }
        };
        let Some(last) = events.last() else { continue };

        let state = yunta_replay::RunState::replay(&events);
        let is_terminal = state.ended();
        if !is_terminal {
            if let Some(places) = old_layouts(ctx, pool, &run_id, &state, collected).await {
                reclaimed.idle.push((run_id.clone(), places));
            }
        }
        let age_days = (now - last.timestamp).num_days();
        if age_days < retention_days as i64 {
            continue;
        }
        if !is_terminal {
            if let Some(tree) = parked_checkout(ctx, pool, &state, collected).await {
                reclaimed.given_back.push((run_id.clone(), tree));
            }
            continue;
        }

        // Death order: files first, rows on a later pass. A run whose
        // run.dir is still on disk (found by its frozen paths) loses only
        // the files this pass; one whose run.dir is already gone (a
        // previous gc, or a human) has its rows purged now.
        match ctx.project.run_dir(run_id.as_str()) {
            Some(run_dir) => {
                let bound = state.run.checkout();
                let tree = worktree_of(&ctx.project, &run_dir, &run_id, bound, collected);
                if let Some(tree) = &tree {
                    if pool_owns(pool, ctx, tree, collected).await == Some(true) {
                        reclaimed.given_back.push((run_id.clone(), tree.clone()));
                    }
                }
                if remove_run(ctx, pool, &run_dir, &run_id, bound, collected).await {
                    collected.reclaimed += 1;
                    if !collected.dry_run {
                        reclaimed.removed.push(run_id.clone());
                    }
                }
            }
            None => {
                if purge_rows(&storage, &run_id, events.len(), collected) {
                    collected.reclaimed += 1;
                }
            }
        }
    }
    Ok(reclaimed)
}

/// Purges the rows of a terminal run whose `run.dir` is already gone, and
/// answers whether they went; under a dry run they only would.
fn purge_rows(storage: &Storage, run: &RunId, events: usize, collected: &mut Collected) -> bool {
    if collected.dry_run {
        let count = surco::text::counted(events, "event");
        collected.did("would purge", format!("{count} of run {}", run.handle()));
        return true;
    }
    match storage.purge_run(run) {
        Ok(purged) => {
            let count = surco::text::counted(purged.rows, "event");
            collected.did("purged", format!("{count} of run {}", run.handle()));
            true
        }
        Err(e) => {
            problem(collected, format!("run {}: {e}", run.handle()));
            false
        }
    }
}

/// Where a run nothing drives kept checkouts before the project's pool:
/// its units' directory under its own, and its own tree when the pool
/// does not keep it. `None` for a run an engine drives now, or may.
async fn old_layouts(
    ctx: &Context,
    pool: &yunta_engine::CheckoutPool,
    run_id: &RunId,
    state: &yunta_replay::RunState,
    collected: &mut Collected,
) -> Option<Vec<PathBuf>> {
    let run_dir = ctx.project.run_dir(run_id.as_str())?;
    let engine = yunta_engine::read_engine(&run_dir, &yunta_engine::lock::SystemProbe);
    if !matches!(
        engine,
        yunta_replay::registry::Engine::Dead | yunta_replay::registry::Engine::Unrecorded
    ) {
        return None;
    }
    let mut places = vec![yunta_engine::run_dir::unit_worktrees(&run_dir)];
    let bound = state.run.checkout();
    if let Some(tree) = worktree_of(&ctx.project, &run_dir, run_id, bound, collected) {
        if pool_owns(pool, ctx, &tree, collected).await == Some(false) {
            places.push(tree);
        }
    }
    Some(places)
}

/// Whether `tree` is a checkout of the project's pool, which a run gives
/// back rather than takes away. `None` when the pool cannot say: the
/// checkout is then left where it is, and the reason is said.
async fn pool_owns(
    pool: &yunta_engine::CheckoutPool,
    ctx: &Context,
    tree: &Path,
    collected: &mut Collected,
) -> Option<bool> {
    match pool.owns(tree, ctx.supervision()).await {
        Ok(owned) => Some(owned),
        Err(e) => {
            let shown = surco::paths::shown(tree, &ctx.cwd, ctx.env.home.as_deref());
            problem(collected, format!("left {shown} alone: {e}"));
            None
        }
    }
}

/// The checkout of the project's pool a parked run works in, when nobody
/// would lose anything if it went back: no session the run left open is
/// waiting to be picked up there. Its work stays on its branch, and a wake
/// takes a checkout again.
async fn parked_checkout(
    ctx: &Context,
    pool: &yunta_engine::CheckoutPool,
    state: &yunta_replay::RunState,
    collected: &mut Collected,
) -> Option<PathBuf> {
    let tree = state.run.checkout()?;
    let continued = state.nodes.iter().any(|(_, record)| {
        matches!(
            record.orphaned_session,
            Some(yunta_core::events::OrphanedSession::Open(_))
        )
    });
    if continued {
        return None;
    }
    (pool_owns(pool, ctx, tree, collected).await == Some(true)).then(|| tree.to_path_buf())
}

/// Removes a terminal run's on-disk footprint — its `run.dir` and, for a
/// worktree-isolated run, the worktree frozen in its manifest — and
/// reports whether every directory that was present got removed. A run
/// with a removal that failed is warned about and returns `false`, so it
/// is never counted as reclaimed while some of its disk survives. Under a
/// dry run nothing is removed and every present directory counts as if
/// it had been.
async fn remove_run(
    ctx: &Context,
    pool: &yunta_engine::CheckoutPool,
    run_dir: &Path,
    run_id: &RunId,
    bound: Option<&Path>,
    collected: &mut Collected,
) -> bool {
    let shown = |dir: &Path| surco::paths::shown(dir, &ctx.cwd, ctx.env.home.as_deref());
    let (mut removed_any, mut all_removed) = (false, true);
    // The worktree goes through git, which unregisters it as well. A
    // checkout of the project's pool is given back, never taken away.
    if let Some(tree) =
        worktree_of(&ctx.project, run_dir, run_id, bound, collected).filter(|tree| tree.exists())
    {
        match pool_owns(pool, ctx, &tree, collected).await {
            Some(true) => {}
            None => all_removed = false,
            Some(false) if collected.dry_run => {
                collected.did("would remove", shown(&tree));
                removed_any = true;
            }
            Some(false) => match yunta_engine::remove_checkout(&tree, ctx.supervision()).await {
                Ok(()) => {
                    collected.did("removed", shown(&tree));
                    removed_any = true;
                }
                Err(e) => {
                    problem(collected, format!("failed to remove {}: {e}", shown(&tree)));
                    all_removed = false;
                }
            },
        }
    }
    if run_dir.exists() {
        if collected.dry_run {
            collected.did("would remove", shown(run_dir));
            removed_any = true;
        } else if let Err(e) = tokio::fs::remove_dir_all(run_dir).await {
            problem(
                collected,
                format!("failed to remove {}: {e}", shown(run_dir)),
            );
            all_removed = false;
        } else {
            collected.did("removed", shown(run_dir));
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
fn worktree_of(
    project: &Project,
    run_dir: &Path,
    run_id: &RunId,
    bound: Option<&Path>,
    collected: &mut Collected,
) -> Option<PathBuf> {
    let manifest: Manifest =
        match crate::load_manifest(&yunta_engine::run_dir::manifest_path(run_dir))
            .map(|manifest| manifest.doc)
        {
            Ok(manifest) => manifest,
            Err(e) => {
                problem(collected, format!("run {}: {e}", run_id.handle()));
                return None;
            }
        };
    match manifest.isolation {
        Isolation::Worktree => Some(
            bound
                .map(Path::to_path_buf)
                .unwrap_or_else(|| project.worktrees_root_for(&manifest).join(run_id.as_str())),
        ),
        Isolation::None => None,
    }
}

/// Something the pass could not do: warned as it happens, and counted on
/// the page the pass ends with.
fn problem(collected: &mut Collected, said: impl std::fmt::Display) {
    warn(said);
    collected.problems += 1;
}
