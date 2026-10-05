//! How checkouts go back to their project's pool: a run's units' and its
//! own when it ends, and the ones a project no longer uses when it
//! collects what is old — and how legacy unit branches are taken in.

use std::path::Path;

use super::super::WorktreeError;
use super::{slot, survey, CheckoutPool};
use crate::lock::Contention;
use crate::process::Supervision;

/// Gives back every checkout the units of `run_id` were kept in, as the
/// run ends: a clean one is kept for nobody, and the refs whose work is
/// in `into` — the run's tree as it closes — go. A blocked task's ref
/// stays, holding what it left, until the run is collected; a checkout
/// with something uncommitted in it stays its unit's.
///
/// What a run made before its project kept checkouts is taken in first:
/// the checkouts it kept under its own directory are taken away, and its
/// units' branches become their refs.
pub async fn release_unit_checkouts(
    pool: &CheckoutPool,
    into: &Path,
    run_dir: &Path,
    run_id: &yunta_core::RunId,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    remove_own_checkouts(into, run_dir, supervision).await?;
    adopt_unit_branches(pool, into, Some(run_id), supervision).await?;
    pool.free_units_of(into, run_id, supervision).await
}

/// Gives back the checkout run `run_id` worked in, as the run ends: it
/// lets go of the run's branch where it stands, unless something in it is
/// uncommitted — somebody's. The branch stays: it holds what the run did.
/// A checkout outside the pool, one a run made before its project kept
/// checkouts, stays where it is.
pub async fn release_run_checkout(
    pool: &CheckoutPool,
    tree: &Path,
    run_id: &yunta_core::RunId,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    let home = pool.home(supervision).await?;
    let Some((number, tree)) = slot::number_of(&home, tree).await else {
        return Ok(());
    };
    let held = slot::hold(&home, number, Contention::Refuse, None, supervision).await?;
    let Some(lease) = held else {
        return Ok(());
    };
    let branch = super::super::run_branch(run_id);
    let lets_go = survey::look(&tree, supervision)
        .await
        .is_some_and(|look| look.clean && look.standing.branch.as_deref() == Some(branch.as_str()));
    if lets_go {
        super::mutating(&tree, &["switch", "--detach", "-q"], supervision).await?;
    }
    pool.give_back(lease, supervision).await;
    Ok(())
}

/// Whether `tree` is still where run `run_id` works: a checkout on the
/// run's own branch, or on a branch of a person's — one who switched it
/// while the run was parked — and never on another run's or unit's.
pub async fn still_the_runs(
    tree: &Path,
    run_id: &yunta_core::RunId,
    supervision: Supervision<'_>,
) -> bool {
    let Some(look) = survey::look(tree, supervision).await else {
        return false;
    };
    let on = look.standing.branch.as_deref().unwrap_or_default();
    on == super::super::run_branch(run_id)
        || !(on.is_empty() || on.starts_with(&format!("{}/", super::super::RUN_BRANCHES)))
            && !super::super::is_legacy_unit_branch(on)
}

/// Hands the checkout a run worked in to the run that carries on from it:
/// the successor's branch is cut where the predecessor's tree stands, in
/// the same checkout, so nothing it built has to be built again.
pub async fn hand_over_run_checkout(
    tree: &Path,
    successor: &yunta_core::RunId,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    let branch = super::super::run_branch(successor);
    super::mutating(tree, &["switch", "-q", "-c", &branch], supervision).await?;
    Ok(())
}

/// Lets go of everything the units of `run_id` kept, landed or not: the
/// run is being collected, and nothing will continue its work. Its units'
/// legacy unit branches go too.
pub async fn forget_run_units(
    pool: &CheckoutPool,
    repo: &Path,
    run_id: &yunta_core::RunId,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    adopt_unit_branches(pool, repo, Some(run_id), supervision).await?;
    pool.forget_units_of(repo, run_id, supervision).await
}

/// Takes in the legacy unit branches — `run`'s, or every run's — as the refs units keep their work by: each branch's commit is kept by
/// its unit's ref, a clean checkout of the pool on it lets it go and is
/// kept for that unit, and the branch goes. A branch checked out where
/// something is uncommitted, held by a live process, or checked out
/// outside the pool, stays. Answers the branches taken in.
pub async fn adopt_unit_branches(
    pool: &CheckoutPool,
    repo: &Path,
    run: Option<&yunta_core::RunId>,
    supervision: Supervision<'_>,
) -> Result<Vec<String>, WorktreeError> {
    let prefix = match run {
        Some(run) => super::super::legacy_unit_branches_of(run),
        None => format!("{}/", super::super::LEGACY_UNIT_BRANCHES),
    };
    let listed = crate::git::output(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            &format!("refs/heads/{prefix}"),
        ],
        supervision,
    )
    .await?;
    let branches: Vec<&str> = listed.lines().filter(|line| !line.is_empty()).collect();
    if branches.is_empty() {
        return Ok(Vec::new());
    }
    let home = pool.home(supervision).await?;
    let listing =
        crate::git::output(repo, &["worktree", "list", "--porcelain"], supervision).await?;
    let mut checked_out = std::collections::HashMap::new();
    for listed in super::super::checkouts(&listing) {
        if let Some(branch) = listed.branch {
            let short = branch.trim_start_matches("refs/heads/").to_string();
            checked_out.insert(short, listed.path);
        }
    }
    let mut taken = Vec::new();
    for branch in branches {
        let Some(unit) = super::super::UnitRef::of_legacy(branch) else {
            continue;
        };
        if let Some(path) = checked_out.get(branch) {
            if !let_go(pool, &home, path, &unit, supervision).await? {
                continue;
            }
        }
        let tip = format!("refs/heads/{branch}");
        crate::git::output(repo, &["update-ref", unit.name(), &tip], supervision).await?;
        super::mutating(repo, &["branch", "-D", branch], supervision).await?;
        taken.push(branch.to_string());
    }
    Ok(taken)
}

/// Lets the checkout at `path` go of the branch it is on, kept for `unit`
/// from here — when it is one of the pool's, nobody holds it, and nothing
/// in it is uncommitted. Answers whether it did.
async fn let_go(
    pool: &CheckoutPool,
    home: &Path,
    path: &Path,
    unit: &super::super::UnitRef,
    supervision: Supervision<'_>,
) -> Result<bool, WorktreeError> {
    let Some((number, path)) = slot::number_of(home, path).await else {
        return Ok(false);
    };
    let held = slot::hold(
        home,
        number,
        Contention::Refuse,
        pool.holder.as_deref(),
        supervision,
    );
    let Some(lease) = held.await? else {
        return Ok(false);
    };
    if !survey::look(&path, supervision)
        .await
        .is_some_and(|look| look.clean)
    {
        return Ok(false);
    }
    super::mutating(&path, &["switch", "--detach", "-q"], supervision).await?;
    lease.keep_for(Some(unit)).await;
    Ok(true)
}

/// Takes away the checkouts a run made under its own directory before its
/// project kept checkouts.
async fn remove_own_checkouts(
    repo: &Path,
    run_dir: &Path,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    let own = crate::run_dir::unit_worktrees(run_dir);
    if !tokio::fs::try_exists(&own).await.unwrap_or(false) {
        return Ok(());
    }
    let own = tokio::fs::canonicalize(&own).await.unwrap_or(own);
    let listing =
        super::super::run_git(repo, &["worktree", "list", "--porcelain"], supervision).await?;
    let common_dir = super::super::common_git_dir(repo, supervision).await?;
    let _mutation_lock =
        super::super::lock_worktree_mutations(&common_dir, supervision.clock).await?;
    for listed in super::super::checkouts(&listing) {
        if listed.path.starts_with(&own) {
            let path = listed.path.display().to_string();
            super::super::run_git(repo, &["worktree", "remove", "--force", &path], supervision)
                .await?;
        }
    }
    Ok(())
}
