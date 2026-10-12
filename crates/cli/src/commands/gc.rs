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

pub async fn gc(dry_run: bool) -> Result<Outcome, CliError> {
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
    let Reclaimed {
        removed,
        given_back,
        idle,
    } = reclaim(&ctx, &pool, dry_run).await?;
    for run_id in &removed {
        forget_units(&ctx, &pool, run_id).await;
    }
    if !dry_run {
        for (run_id, tree) in &given_back {
            give_back(&ctx, &pool, run_id, tree).await;
        }
        adopt(&ctx, &pool).await;
        for (run_id, places) in &idle {
            take_old_layouts(&ctx, run_id, places).await;
        }
        tidy(&ctx, &pool).await;
    } else {
        would_adopt(&ctx, &pool).await;
        if let Ok(kept) = pool.report(ctx.supervision()).await {
            println!("the project's checkouts: {kept}");
        }
    }
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
async fn give_back(ctx: &Context, pool: &yunta_engine::CheckoutPool, run_id: &RunId, tree: &Path) {
    let released = yunta_engine::release_run_checkout(pool, tree, run_id, ctx.supervision());
    if let Err(e) = released.await {
        warn(format!("run {}: {e}", run_id.handle()));
    }
}

/// What git still holds of a run whose files are gone: the checkouts its
/// units worked in, and the branches they worked on — a blocked task's
/// left work among them, which nothing reads once the run is collected.
/// The project's checkouts stay, with no branch, for the runs to come.
async fn forget_units(ctx: &Context, pool: &yunta_engine::CheckoutPool, run_id: &RunId) {
    let supervision = ctx.supervision();
    if let Err(e) = yunta_engine::prune_worktrees(&ctx.cwd, supervision).await {
        warn(format!("run {}: {e}", run_id.handle()));
        return;
    }
    if let Err(e) = yunta_engine::forget_run_units(pool, &ctx.cwd, run_id, supervision).await {
        warn(format!("run {}: {e}", run_id.handle()));
    }
}

/// Takes in the units' branches runs made before refs kept their work:
/// what each unit committed stays, kept by its ref, and no list of the
/// project's branches shows it.
async fn adopt(ctx: &Context, pool: &yunta_engine::CheckoutPool) {
    match yunta_engine::adopt_unit_branches(pool, &ctx.cwd, None, ctx.supervision()).await {
        Ok(taken) if !taken.is_empty() => {
            let branches =
                yunta_core::text::counted_as(taken.len(), "unit branch", "unit branches");
            println!("kept the work of {branches} by refs");
        }
        Ok(_) => {}
        Err(e) => warn(format!("the units' branches: {e}")),
    }
}

/// Says which of the units' branches runs made before refs kept their work
/// a pass would take in, taking none.
async fn would_adopt(ctx: &Context, pool: &yunta_engine::CheckoutPool) {
    match yunta_engine::adoptable_unit_branches(pool, &ctx.cwd, ctx.supervision()).await {
        Ok(branches) if !branches.is_empty() => {
            let branches =
                yunta_core::text::counted_as(branches.len(), "unit branch", "unit branches");
            println!("would keep the work of {branches} by refs");
        }
        Ok(_) => {}
        Err(e) => warn(format!("the units' branches: {e}")),
    }
}

/// Tidies the project's pool — what its checkouts' builds take is
/// measured where nobody did, a build unused within retention is emptied,
/// the builds stay within the budget, and a free checkout holding no
/// build goes — and says what the pool keeps.
async fn tidy(ctx: &Context, pool: &yunta_engine::CheckoutPool) {
    let unused = ctx
        .project
        .config
        .storage
        .as_ref()
        .and_then(|storage| storage.retention_days)
        .map(|days| chrono::Duration::days(i64::from(days)));
    match pool.tidy(unused, ctx.supervision()).await {
        Ok(tidied) => {
            for (checkout, bytes) in &tidied.emptied {
                println!(
                    "emptied the build of {} ({})",
                    checkout.display(),
                    yunta_core::units::Bytes(*bytes)
                );
            }
            for checkout in &tidied.removed {
                println!("removed the unused checkout {}", checkout.display());
            }
            println!("the project's checkouts: {}", tidied.kept);
        }
        Err(e) => warn(format!("the project's checkouts: {e}")),
    }
}

/// Takes away the clean checkouts run `run_id`, which nothing drives,
/// kept at `places` before the project's pool, and names the ones kept.
async fn take_old_layouts(ctx: &Context, run_id: &RunId, places: &[PathBuf]) {
    for place in places {
        match yunta_engine::remove_old_checkouts(&ctx.cwd, place, ctx.supervision()).await {
            Ok(done) => {
                for checkout in done {
                    match checkout {
                        yunta_engine::OldCheckout::Removed(path) => {
                            println!("removed {} of run {}", path.display(), run_id.handle());
                        }
                        yunta_engine::OldCheckout::Kept(path) => println!(
                            "kept {} of run {}: something in it is uncommitted",
                            path.display(),
                            run_id.handle()
                        ),
                    }
                }
            }
            Err(e) => warn(format!("run {}: {e}", run_id.handle())),
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
    dry_run: bool,
) -> Result<Reclaimed, CliError> {
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
        return Ok(Reclaimed::default());
    };

    let storage = ctx.storage()?;
    let run_ids = storage
        .list_runs()
        .map(|runs| runs.into_iter().map(|run| run.run_id).collect::<Vec<_>>())?;

    let now = yunta_core::Clock::now(&ctx.clock);
    let mut reclaimed = 0usize;
    let mut collected = Reclaimed::default();
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
        let is_terminal = state.ended();
        if !is_terminal {
            if let Some(places) = old_layouts(ctx, pool, &run_id, &state).await {
                collected.idle.push((run_id.clone(), places));
            }
        }
        let age_days = (now - last.timestamp).num_days();
        if age_days < retention_days as i64 {
            continue;
        }
        if !is_terminal {
            if let Some(tree) = parked_checkout(ctx, pool, &state).await {
                collected.given_back.push((run_id.clone(), tree));
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
                let tree = worktree_of(&ctx.project, &run_dir, &run_id, bound);
                if let Some(tree) = &tree {
                    if pool_owns(pool, ctx, tree).await == Some(true) {
                        collected.given_back.push((run_id.clone(), tree.clone()));
                    }
                }
                if remove_run(ctx, pool, &run_dir, &run_id, bound, dry_run).await {
                    reclaimed += 1;
                    if !dry_run {
                        collected.removed.push(run_id.clone());
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
    Ok(collected)
}

/// Where a run nothing drives kept checkouts before the project's pool:
/// its units' directory under its own, and its own tree when the pool
/// does not keep it. `None` for a run an engine drives now, or may.
async fn old_layouts(
    ctx: &Context,
    pool: &yunta_engine::CheckoutPool,
    run_id: &RunId,
    state: &yunta_engine::RunState,
) -> Option<Vec<PathBuf>> {
    let run_dir = ctx.project.run_dir(run_id.as_str())?;
    let liveness = yunta_engine::engine_liveness(&run_dir, &yunta_engine::lock::SystemProbe);
    if !matches!(
        liveness,
        yunta_engine::EngineLiveness::Dead | yunta_engine::EngineLiveness::Unrecorded
    ) {
        return None;
    }
    let mut places = vec![yunta_engine::run_dir::unit_worktrees(&run_dir)];
    let bound = state.run.checkout();
    if let Some(tree) = worktree_of(&ctx.project, &run_dir, run_id, bound) {
        if pool_owns(pool, ctx, &tree).await == Some(false) {
            places.push(tree);
        }
    }
    Some(places)
}

/// Whether `tree` is a checkout of the project's pool, which a run gives
/// back rather than takes away. `None` when the pool cannot say: the
/// checkout is then left where it is, and the reason is said.
async fn pool_owns(pool: &yunta_engine::CheckoutPool, ctx: &Context, tree: &Path) -> Option<bool> {
    match pool.owns(tree, ctx.supervision()).await {
        Ok(owned) => Some(owned),
        Err(e) => {
            let shown = crate::render::paths::shown(tree, &ctx.cwd, ctx.env.home.as_deref());
            warn(format!("left {shown} alone: {e}"));
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
    state: &yunta_engine::RunState,
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
    (pool_owns(pool, ctx, tree).await == Some(true)).then(|| tree.to_path_buf())
}

/// Removes a terminal run's on-disk footprint — its `run.dir` and, for a
/// worktree-isolated run, the worktree frozen in its manifest — and
/// reports whether every directory that was present got removed. A run
/// with a removal that failed is warned about and returns `false`, so it
/// is never counted as reclaimed while some of its disk survives. Under
/// `dry_run` nothing is removed and every present directory counts as if
/// it had been.
async fn remove_run(
    ctx: &Context,
    pool: &yunta_engine::CheckoutPool,
    run_dir: &Path,
    run_id: &RunId,
    bound: Option<&Path>,
    dry_run: bool,
) -> bool {
    let shown = |dir: &Path| crate::render::paths::shown(dir, &ctx.cwd, ctx.env.home.as_deref());
    let mut removed_any = false;
    let mut all_removed = true;

    // The worktree goes through git, which unregisters it as well. A
    // checkout of the project's pool is given back, never taken away.
    if let Some(tree) =
        worktree_of(&ctx.project, run_dir, run_id, bound).filter(|tree| tree.exists())
    {
        match pool_owns(pool, ctx, &tree).await {
            Some(true) => {}
            None => all_removed = false,
            Some(false) if dry_run => {
                println!("would remove {}", shown(&tree));
                removed_any = true;
            }
            Some(false) => match yunta_engine::remove_checkout(&tree, ctx.supervision()).await {
                Ok(()) => {
                    println!("removed {}", shown(&tree));
                    removed_any = true;
                }
                Err(e) => {
                    warn(format!("failed to remove {}: {e}", shown(&tree)));
                    all_removed = false;
                }
            },
        }
    }
    if run_dir.exists() {
        if dry_run {
            println!("would remove {}", shown(run_dir));
            removed_any = true;
        } else if let Err(e) = std::fs::remove_dir_all(run_dir) {
            warn(format!("failed to remove {}: {e}", shown(run_dir)));
            all_removed = false;
        } else {
            println!("removed {}", shown(run_dir));
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
) -> Option<PathBuf> {
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
        Isolation::Worktree => Some(
            bound
                .map(Path::to_path_buf)
                .unwrap_or_else(|| project.worktrees_root_for(&manifest).join(run_id.as_str())),
        ),
        Isolation::None => None,
    }
}
