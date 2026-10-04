//! How checkouts go back to their project's pool: a unit's when its work
//! lands, a run's when it ends, and the ones a project no longer uses when
//! it collects what is old.

use std::path::{Path, PathBuf};

use super::super::{head_commit, run_units, Unit, WorktreeError};
use super::{slot, CheckoutPool};
use crate::lock::Contention;
use crate::process::Supervision;

/// Lets go of `unit`'s branch once its work landed on `into`: the checkout
/// stays where it stands with no branch — free for the pool — and the
/// branch goes, since `into` holds its work.
pub async fn retire_unit_branch(
    unit: &Unit,
    into: &Path,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    let on = crate::git::output(&unit.worktree, &["branch", "--show-current"], supervision).await?;
    let branch = on.trim();
    if branch.is_empty() {
        return Ok(());
    }
    crate::git::output(&unit.worktree, &["switch", "--detach", "-q"], supervision).await?;
    if let Err(error) = crate::git::output(into, &["branch", "-d", branch], supervision).await {
        tracing::debug!(%error, branch, "a landed unit's branch stays");
    }
    Ok(())
}

/// Gives back every checkout the units of `run_id` still hold, as the run
/// ends: each lets go of its unit's branch, and a branch whose work is in
/// `into` — the run's tree as it closes — goes with it. A branch holding
/// work that never landed — a blocked task's — stays for whoever wants it,
/// until the run is collected. A checkout with something uncommitted in it
/// stays its unit's.
///
/// A run made before its project kept checkouts kept them under its own
/// directory; those are taken away as they always were.
pub async fn release_unit_checkouts(
    pool: &CheckoutPool,
    into: &Path,
    run_dir: &Path,
    run_id: &yunta_core::RunId,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    let landed = head_commit(into, supervision).await?;
    remove_own_checkouts(into, run_dir, supervision).await?;
    let ours = format!("{}/", run_units(run_id));
    let home = pool.home(supervision).await?;
    let kept = let_go(&home, &ours, pool.holder.as_deref(), supervision).await?;
    for branch in branches(into, &ours, supervision).await? {
        let landed_there = crate::git::success(
            into,
            &["merge-base", "--is-ancestor", &branch, landed.as_str()],
            supervision,
        )
        .await?;
        if landed_there && kept.iter().all(|still| still != &branch) {
            super::super::run_git(into, &["branch", "-D", &branch], supervision).await?;
        }
    }
    Ok(())
}

/// Lets go of every branch of `run_id`'s units in the pool and deletes
/// them all, landed or not: the run is being collected, and nothing will
/// continue its work.
pub async fn forget_run_units(
    pool: &CheckoutPool,
    repo: &Path,
    run_id: &yunta_core::RunId,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    let ours = format!("{}/", run_units(run_id));
    let home = pool.home(supervision).await?;
    let kept = let_go(&home, &ours, pool.holder.as_deref(), supervision).await?;
    for branch in branches(repo, &ours, supervision).await? {
        if kept.iter().all(|still| still != &branch) {
            super::super::run_git(repo, &["branch", "-D", &branch], supervision).await?;
        }
    }
    Ok(())
}

/// Takes away the free checkouts of the pool beyond as many as were busy
/// at once lately, the least recently used first: a project keeps the
/// warm checkouts it uses, and no more.
pub async fn trim_pool(
    pool: &CheckoutPool,
    supervision: Supervision<'_>,
) -> Result<Vec<PathBuf>, WorktreeError> {
    let home = pool.home(supervision).await?;
    let keep = busiest(&home, supervision.clock.now()).await;
    let mut free = Vec::new();
    for (number, path) in slot::listed(&home).await {
        if !slot::is_checkout(&path, supervision).await {
            continue;
        }
        let on = crate::git::output(&path, &["branch", "--show-current"], supervision).await;
        if on.is_ok_and(|on| on.trim().is_empty()) && slot::is_clean(&path, supervision).await {
            let used = slot::last_held(&home, number).await.map(|held| held.at);
            free.push((std::cmp::Reverse(used), number, path));
        }
    }
    free.sort();
    let mut removed = Vec::new();
    for (_, number, path) in free.into_iter().skip(keep) {
        let held = slot::hold(&home, number, Contention::Refuse, None, supervision);
        let Some(_lease) = held.await? else {
            continue;
        };
        let common_dir = super::super::common_git_dir(&pool.repo, supervision).await?;
        let _mutation_lock =
            super::super::lock_worktree_mutations(&common_dir, supervision.clock).await?;
        let shown = path.display().to_string();
        super::super::run_git(
            &pool.repo,
            &["worktree", "remove", "--force", &shown],
            supervision,
        )
        .await?;
        removed.push(path);
    }
    Ok(removed)
}

/// How many checkouts were busy at once within the time the pool
/// remembers, as the pool last recorded it — every checkout it has, when
/// it recorded nothing.
async fn busiest(home: &Path, now: chrono::DateTime<chrono::Utc>) -> usize {
    match slot::busiest(home).await {
        Some((count, at)) if now - at <= slot::remembered() => count.max(1),
        Some(_) => 1,
        None => usize::MAX,
    }
}

/// Lets every clean checkout under `home` on a branch starting with
/// `ours` go of it. Answers the branches still held: by a checkout with
/// something uncommitted, or held by a live process.
async fn let_go(
    home: &Path,
    ours: &str,
    holder: Option<&Path>,
    supervision: Supervision<'_>,
) -> Result<Vec<String>, WorktreeError> {
    let mut kept = Vec::new();
    for (number, path) in slot::listed(home).await {
        let Ok(on) = crate::git::output(&path, &["branch", "--show-current"], supervision).await
        else {
            continue;
        };
        let branch = on.trim().to_string();
        if !branch.starts_with(ours) {
            continue;
        }
        let held = slot::hold(home, number, Contention::Refuse, holder, supervision).await?;
        match held {
            Some(_lease) if slot::is_clean(&path, supervision).await => {
                crate::git::output(&path, &["switch", "--detach", "-q"], supervision).await?;
            }
            _ => kept.push(branch),
        }
    }
    Ok(kept)
}

/// The branches of `repo` starting with `prefix`.
async fn branches(
    repo: &Path,
    prefix: &str,
    supervision: Supervision<'_>,
) -> Result<Vec<String>, WorktreeError> {
    let refs = format!("refs/heads/{prefix}");
    let listed = super::super::run_git(
        repo,
        &["for-each-ref", "--format=%(refname:short)", &refs],
        supervision,
    )
    .await?;
    Ok(listed
        .lines()
        .map(str::trim)
        .filter(|branch| !branch.is_empty())
        .map(str::to_string)
        .collect())
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
    for (checkout, _) in super::super::checkouts(&listing) {
        if checkout.starts_with(&own) {
            let path = checkout.display().to_string();
            super::super::run_git(repo, &["worktree", "remove", "--force", &path], supervision)
                .await?;
        }
    }
    Ok(())
}
