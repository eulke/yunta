//! The checkout a run works in itself: taken from its project's pool on a
//! branch of the run's own, which keeps it the run's for as long as the
//! run lives, and taken again on that branch when a wake has to.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use yunta_core::CommitSha;

use super::{reset, CheckoutPool, Fresh, Lease};
use crate::process::Supervision;
use crate::worktree::WorktreeError;

impl CheckoutPool {
    /// A checkout of the pool for run `run_id` to work in, on the run's own
    /// branch cut from `base`: a free one put back to that commit, or a new
    /// one when none is free. The run's branch keeps it the run's for as
    /// long as the run lives; the hold handed back only covers its taking.
    pub async fn open_run(
        self: &Arc<Self>,
        run_id: &yunta_core::RunId,
        base: &CommitSha,
        supervision: Supervision<'_>,
    ) -> Result<(PathBuf, Lease), WorktreeError> {
        let branch = crate::worktree::run_branch(run_id);
        match self.take(Some(&branch), base, supervision).await? {
            Some((checkout, lease)) => {
                reset(&checkout, &branch, base, supervision).await?;
                Ok((checkout, lease))
            }
            None => {
                let repo = self.repo.clone();
                let fresh = Fresh::Cut {
                    branch: &branch,
                    base,
                };
                self.add(&repo, fresh, supervision).await
            }
        }
    }

    /// The checkout run `run_id` works in again, on the branch that holds
    /// its work: the one still on that branch, or else a free one switched
    /// to it, or a new one.
    pub async fn reopen_run(
        self: &Arc<Self>,
        run_id: &yunta_core::RunId,
        supervision: Supervision<'_>,
    ) -> Result<(PathBuf, Lease), WorktreeError> {
        let branch = crate::worktree::run_branch(run_id);
        let head = crate::git::output(&self.repo, &["rev-parse", &branch], supervision).await?;
        let head: CommitSha = head
            .trim()
            .parse()
            .map_err(|source| WorktreeError::NotACommit {
                args: format!("rev-parse {branch}"),
                cwd: self.repo.clone(),
                source,
            })?;
        match self.take(Some(&branch), &head, supervision).await? {
            Some((checkout, lease)) => {
                let on = crate::git::output(&checkout, &["branch", "--show-current"], supervision)
                    .await?;
                if on.trim() != branch {
                    let switch = ["switch", "--discard-changes", "-q", branch.as_str()];
                    super::mutating(&checkout, &switch, supervision).await?;
                    crate::git::output(&checkout, &["clean", "-q", "-ffd"], supervision).await?;
                }
                Ok((checkout, lease))
            }
            None => {
                let repo = self.repo.clone();
                self.add(&repo, Fresh::On { branch: &branch }, supervision)
                    .await
            }
        }
    }

    /// Whether `checkout` is one of the pool's.
    pub async fn keeps(&self, checkout: &Path, supervision: Supervision<'_>) -> bool {
        let Ok(home) = self.home(supervision).await else {
            return false;
        };
        let checkout = tokio::fs::canonicalize(checkout)
            .await
            .unwrap_or_else(|_| checkout.to_path_buf());
        checkout.parent() == Some(home.as_path())
    }
}
