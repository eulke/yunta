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
    let Reclaimed {
        removed,
        given_back,
    } = reclaim(&ctx, dry_run)?;
    let pool = yunta_engine::CheckoutPool::upkeep(&ctx.project.worktrees_root, &ctx.cwd);
    for run_id in &removed {
        forget_units(&ctx, &pool, run_id).await;
    }
    if !dry_run {
        for (run_id, tree) in &given_back {
            give_back(&ctx, &pool, run_id, tree).await;
        }
        trim(&ctx, &pool).await;
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
    if let Err(e) = yunta_engine::git::output(&ctx.cwd, &["worktree", "prune"], supervision).await {
        warn(format!("run {}: {e}", run_id.handle()));
        return;
    }
    if let Err(e) = yunta_engine::forget_run_units(pool, &ctx.cwd, run_id, supervision).await {
        warn(format!("run {}: {e}", run_id.handle()));
    }
}

/// Takes away the project's free checkouts beyond as many as its runs had
/// busy at once lately.
async fn trim(ctx: &Context, pool: &yunta_engine::CheckoutPool) {
    match yunta_engine::trim_pool(pool, ctx.supervision()).await {
        Ok(removed) => {
            for checkout in removed {
                println!("removed the unused checkout {}", checkout.display());
            }
        }
        Err(e) => warn(format!("the project's checkouts: {e}")),
    }
}

/// Removes every terminal run past retention — its files on this pass,
/// its rows on a later one — and answers the runs whose files went, with
/// the checkouts of the project's pool to give back: a collected run's,
/// and that of a parked run nobody came back to within retention.
fn reclaim(ctx: &Context, dry_run: bool) -> Result<Reclaimed, CliError> {
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
        let is_terminal = state.broken.is_some()
            || events.iter().any(|e| {
                matches!(
                    e.payload(),
                    Some(yunta_core::events::EventPayload::Run(RunEvent::Finished(_)))
                )
            });
        let age_days = (now - last.timestamp).num_days();
        if age_days < retention_days as i64 {
            continue;
        }
        if !is_terminal {
            if let Some(tree) = parked_checkout(&state) {
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
                if let Some(tree) = tree.as_ref().filter(|tree| in_the_pool(tree)) {
                    collected.given_back.push((run_id.clone(), tree.clone()));
                }
                if remove_run(ctx, &run_dir, &run_id, bound, dry_run) {
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

/// Whether `tree` is a checkout of a project's pool — `pool/<project>/slot-N`
/// under whichever worktrees root the run froze — which a run gives back
/// rather than takes away.
fn in_the_pool(tree: &Path) -> bool {
    let named = |path: Option<&Path>, what: &dyn Fn(&str) -> bool| {
        path.and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .is_some_and(what)
    };
    named(Some(tree), &|name| name.starts_with("slot-"))
        && named(tree.parent().and_then(Path::parent), &|name| name == "pool")
}

/// The checkout of the project's pool a parked run works in, when nobody
/// would lose anything if it went back: no session the run left open is
/// waiting to be picked up there. Its work stays on its branch, and a wake
/// takes a checkout again.
fn parked_checkout(state: &yunta_engine::RunState) -> Option<PathBuf> {
    let tree = state.run.checkout()?;
    let continued = state.nodes.iter().any(|(_, record)| {
        matches!(
            record.orphaned_session,
            Some(yunta_core::events::OrphanedSession::Open(_))
        )
    });
    (!continued && in_the_pool(tree)).then(|| tree.to_path_buf())
}

/// Removes a terminal run's on-disk footprint — its `run.dir` and, for a
/// worktree-isolated run, the worktree frozen in its manifest — and
/// reports whether every directory that was present got removed. A run
/// with a removal that failed is warned about and returns `false`, so it
/// is never counted as reclaimed while some of its disk survives. Under
/// `dry_run` nothing is removed and every present directory counts as if
/// it had been.
fn remove_run(
    ctx: &Context,
    run_dir: &Path,
    run_id: &RunId,
    bound: Option<&Path>,
    dry_run: bool,
) -> bool {
    let worktree = worktree_of(&ctx.project, run_dir, run_id, bound);
    let shown = |dir: &Path| crate::render::paths::shown(dir, &ctx.cwd, ctx.env.home.as_deref());
    let mut removed_any = false;
    let mut all_removed = true;

    // A checkout of the project's pool is given back, never taken away.
    let worktree = worktree.filter(|tree| !in_the_pool(tree));
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
