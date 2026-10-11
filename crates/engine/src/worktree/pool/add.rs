//! Adding a checkout to the pool, when none of those it keeps fits its
//! next user.

use std::path::{Path, PathBuf};

use super::slot::{self, SLOT};
use super::{survey, CheckoutPool, Fresh, Lease, Located};
use crate::lock::Contention;
use crate::process::Supervision;
use crate::worktree::WorktreeError;

impl CheckoutPool {
    /// Adds a checkout to the pool under the lowest number nobody uses,
    /// held for its caller.
    pub(super) async fn add(
        &self,
        listed: std::collections::HashSet<u32>,
        repo: &Path,
        fresh: Fresh<'_>,
        supervision: Supervision<'_>,
    ) -> Result<(PathBuf, Lease), WorktreeError> {
        let Located { home, common_dir } = self.located(supervision).await?;
        let home = home.clone();
        tokio::fs::create_dir_all(&home)
            .await
            .map_err(|source| WorktreeError::Io {
                action: "create the project's checkouts directory".to_string(),
                path: home.clone(),
                source,
            })?;
        let mut number = 0;
        loop {
            number += 1;
            if listed.contains(&number) {
                continue;
            }
            let path = home.join(format!("{SLOT}{number}"));
            let held = slot::hold(
                &home,
                number,
                Contention::Refuse,
                self.holder.as_deref(),
                supervision,
            );
            let Some(lease) = held.await? else {
                continue;
            };
            if tokio::fs::try_exists(&path).await.unwrap_or(false) {
                // Added by another process since the survey, or left half
                // added by one that crashed.
                if survey::look(&path, supervision).await.is_some() {
                    continue;
                }
                slot::clear(repo, &path, supervision).await?;
            }
            make(repo, common_dir, &path, fresh, supervision).await?;
            // A checkout just added holds its source and no build.
            super::space::emptied(&home, number, supervision.clock.now());
            let path = tokio::fs::canonicalize(&path).await.unwrap_or(path);
            return Ok((path, lease));
        }
    }
}

/// Makes the checkout of `repo` at `path` that `fresh` says, every other
/// change to the project's checkouts — whose common git directory is
/// `common_dir` — held off while it is added. One cut on a new branch
/// that is stopped half way leaves neither the checkout nor the branch.
async fn make(
    repo: &Path,
    common_dir: &Path,
    path: &Path,
    fresh: Fresh<'_>,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    match fresh {
        Fresh::Cut { branch, base } => {
            let cut = ["-b", branch, base.as_str()];
            match add(repo, common_dir, path, &cut, supervision).await {
                Err(stopped) if stopped.cancelled() => {
                    crate::worktree::undo_half_added(repo, path, branch, supervision).await;
                    Err(stopped)
                }
                added => added,
            }
        }
        Fresh::On { branch } => add(repo, common_dir, path, &[branch], supervision).await,
        Fresh::Detached { base } => {
            let detached = ["--detach", base.as_str()];
            add(repo, common_dir, path, &detached, supervision).await
        }
    }
}

/// Adds a checkout of `repo` at `checkout`, on what `on` names: a branch,
/// or `--detach` and a commit.
async fn add(
    repo: &Path,
    common_dir: &Path,
    checkout: &Path,
    on: &[&str],
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    if let Some(parent) = checkout.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|source| WorktreeError::Io {
                action: "create the worktrees directory".to_string(),
                path: parent.to_path_buf(),
                source,
            })?;
    }
    let _mutation_lock =
        crate::worktree::lock_worktree_mutations(common_dir, supervision.clock).await?;
    let path = checkout.display().to_string();
    let mut args = vec!["worktree", "add", path.as_str()];
    args.extend_from_slice(on);
    crate::worktree::run_git(repo, &args, supervision).await?;
    Ok(())
}
