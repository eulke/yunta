//! Adding a checkout to the pool, when none of those it keeps fits its
//! next user.

use std::path::{Path, PathBuf};

use super::slot::{self, clearable, SLOT};
use super::{CheckoutPool, Fresh, Lease, Located};
use crate::git::{self, MutationLock};
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
        create_home(&home).await?;
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
                // Left half added by a process that crashed, or cut off half
                // way, it is no checkout, and goes before it is made again;
                // otherwise another process added it since the survey.
                if clearable(&home, number, &path).await {
                    slot::clear(repo, &path, supervision).await?;
                } else {
                    git::tree_state(&path, &[], supervision).await?;
                    continue;
                }
            }
            make(repo, common_dir, &path, fresh, supervision).await?;
            // A checkout just added holds its source and no build.
            super::space::emptied(&home, number, supervision.clock.now());
            let path = tokio::fs::canonicalize(&path).await.unwrap_or(path);
            return Ok((path, lease));
        }
    }
}

/// Creates the pool's directory at `home`, where its checkouts are added.
async fn create_home(home: &Path) -> Result<(), WorktreeError> {
    tokio::fs::create_dir_all(home)
        .await
        .map_err(|source| WorktreeError::Io {
            action: "create the project's checkouts directory".to_string(),
            path: home.to_path_buf(),
            source,
        })
}

/// Makes the checkout of `repo` at `path` that `fresh` says, every other
/// change to the project's checkouts — whose common git directory is
/// `common_dir` — held off while it is added, and while an add stopped half
/// way is undone. An add stopped half way leaves neither the checkout nor
/// git's record of it, nor the branch a cut made.
async fn make(
    repo: &Path,
    common_dir: &Path,
    path: &Path,
    fresh: Fresh<'_>,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    let held = git::mutation_lock(common_dir, supervision)
        .await
        .map_err(WorktreeError::Lock)?;
    let (on, made) = match fresh {
        Fresh::Cut { branch, base } => (vec!["-b", branch, base.as_str()], Some(branch)),
        Fresh::On { branch } => (vec![branch], None),
        Fresh::Detached { base } => (vec!["--detach", base.as_str()], None),
    };
    match add(repo, &held, path, &on, supervision).await {
        Err(stopped) if stopped.cancelled() => {
            crate::worktree::undo_half_added(&held, repo, path, made, supervision).await;
            Err(stopped)
        }
        added => added,
    }
}

/// Adds a checkout of `repo` at `checkout`, on what `on` names: a branch,
/// or `--detach` and a commit. `held` is the mutation lock the caller takes
/// for it.
async fn add(
    repo: &Path,
    held: &MutationLock,
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
    let path = checkout.display().to_string();
    let mut args = vec!["worktree", "add", path.as_str()];
    args.extend_from_slice(on);
    git::change_held(held, repo, &args, supervision).await?;
    Ok(())
}
