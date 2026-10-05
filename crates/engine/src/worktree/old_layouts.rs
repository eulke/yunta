//! Checkouts runs kept before the project's pool: a run's units' under
//! its own directory, and a run's own tree named after it under the
//! worktrees root. A run nothing drives does not need them — a wake takes
//! its checkouts from the pool, and a ref or a branch keeps any work they
//! committed — so `gc` takes the clean ones away and names the rest.

use std::path::{Path, PathBuf};

use super::{checkouts, common_git_dir, is_clean, lock_worktree_mutations, run_git, WorktreeError};
use crate::process::Supervision;

/// What became of one checkout of the earlier layouts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OldCheckout {
    /// Taken away: nothing in it was uncommitted.
    Removed(PathBuf),
    /// Kept: something in it is uncommitted, somebody's.
    Kept(PathBuf),
}

/// Takes away every checkout of `repo` at or under `under` that nothing
/// is uncommitted in, and names the ones it kept.
pub async fn remove_old_checkouts(
    repo: &Path,
    under: &Path,
    supervision: Supervision<'_>,
) -> Result<Vec<OldCheckout>, WorktreeError> {
    let Ok(under) = tokio::fs::canonicalize(under).await else {
        return Ok(Vec::new());
    };
    let listing = run_git(repo, &["worktree", "list", "--porcelain"], supervision).await?;
    let mut done = Vec::new();
    // The first checkout git lists is the repository's own, never a run's.
    for listed in checkouts(&listing).into_iter().skip(1) {
        let path = tokio::fs::canonicalize(&listed.path)
            .await
            .unwrap_or(listed.path);
        if !path.starts_with(&under) || listed.prunable {
            continue;
        }
        if !is_clean(&path, supervision).await? {
            done.push(OldCheckout::Kept(path));
            continue;
        }
        let common_dir = common_git_dir(repo, supervision).await?;
        let _mutation_lock = lock_worktree_mutations(&common_dir, supervision.clock).await?;
        let shown = path.display().to_string();
        run_git(
            repo,
            &["worktree", "remove", "--force", &shown],
            supervision,
        )
        .await?;
        done.push(OldCheckout::Removed(path));
    }
    Ok(done)
}
