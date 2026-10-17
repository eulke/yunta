//! Checkouts runs kept before the project's pool: a run's units' under
//! its own directory, and a run's own tree named after it under the
//! worktrees root. A run nothing drives does not need them — a wake takes
//! its checkouts from the pool, and a ref or a branch keeps any work they
//! committed — so `gc` takes the clean ones away and names the rest.

use std::path::{Path, PathBuf};

use super::{checkouts, WorktreeError};
use crate::git;
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
    let listing = git::read(repo, &["worktree", "list", "--porcelain"], supervision).await?;
    let mut done = Vec::new();
    // The first checkout git lists is the repository's own, never a run's.
    for listed in checkouts(&listing).into_iter().skip(1) {
        let path = tokio::fs::canonicalize(&listed.path)
            .await
            .unwrap_or(listed.path);
        if !path.starts_with(&under) || listed.prunable {
            continue;
        }
        let common_dir = git::common_dir(repo, supervision).await?;
        let held = git::mutation_lock(&common_dir, supervision)
            .await
            .map_err(WorktreeError::Lock)?;
        // Judged under the lock the removal takes: what is uncommitted
        // then is what the removal would lose.
        if !git::tree_state(&path, &[], supervision).await?.clean {
            done.push(OldCheckout::Kept(path));
            continue;
        }
        let shown = path.display().to_string();
        git::change_held(
            &held,
            repo,
            &["worktree", "remove", "--force", &shown],
            supervision,
        )
        .await?;
        done.push(OldCheckout::Removed(path));
    }
    Ok(done)
}
