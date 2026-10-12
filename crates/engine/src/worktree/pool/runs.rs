//! The checkout a run works in itself: taken from its project's pool on a
//! branch of the run's own, which keeps it the run's for as long as the
//! run lives, and taken again on that branch when a wake has to.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use yunta_core::CommitSha;

use super::{fit, reset, CheckoutPool, Fresh, Lease, Taken};
use crate::process::Supervision;
use crate::worktree::WorktreeError;

/// Which free checkout of the pool a run is born in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Birth {
    /// The run builds in its own tree: it is born in the free checkout
    /// holding the most of a build.
    Builds,
    /// Units do the run's work, each in a checkout of its own, and the run
    /// moves into the build the last one leaves: it is born in the free
    /// checkout holding the least, which leaves the warm ones to its units
    /// and to its measurement.
    FollowsUnits,
}

impl Birth {
    /// How a run of `workflow` is born: following its units when a node of
    /// it lands units' work.
    pub fn of(workflow: &yunta_core::Workflow) -> Birth {
        match workflow.iter_nodes().any(crate::worktree::lands_units) {
            true => Birth::FollowsUnits,
            false => Birth::Builds,
        }
    }
}

impl CheckoutPool {
    /// A checkout of the pool for run `run_id` to work in, on the run's own
    /// branch cut from `base`: a free one put back to that commit, chosen as
    /// `birth` says, or a new one when none is free. The run's branch keeps
    /// it the run's for as long as the run lives; the hold handed back only
    /// covers its taking.
    pub async fn open_run(
        self: &Arc<Self>,
        run_id: &yunta_core::RunId,
        base: &CommitSha,
        birth: Birth,
        supervision: Supervision<'_>,
    ) -> Result<(PathBuf, Lease), WorktreeError> {
        let branch = crate::worktree::run_branch(run_id);
        let user = fit::User::Run {
            branch: &branch,
            birth,
        };
        match Box::pin(self.take(user, base, supervision)).await? {
            Taken::Held(checkout, lease) => {
                reset(&checkout, Some(&branch), base, supervision).await?;
                Ok((checkout, lease))
            }
            Taken::Nothing { listed } => {
                let repo = self.repo.clone();
                let fresh = Fresh::Cut {
                    branch: &branch,
                    base,
                };
                self.add(listed, &repo, fresh, supervision).await
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
        let head = crate::git::read(&self.repo, &["rev-parse", &branch], supervision).await?;
        let head: CommitSha = head
            .trim()
            .parse()
            .map_err(|source| WorktreeError::NotACommit {
                args: format!("rev-parse {branch}"),
                cwd: self.repo.clone(),
                source,
            })?;
        // The checkout still on the run's branch goes first; past it, the
        // warmest at the run's commit — the one the run gave back while
        // parked, holding its build, when nobody took it since.
        let user = fit::User::Run {
            branch: &branch,
            birth: Birth::Builds,
        };
        match Box::pin(self.take(user, &head, supervision)).await? {
            Taken::Held(checkout, lease) => {
                let on =
                    crate::git::read(&checkout, &["branch", "--show-current"], supervision).await?;
                if on.trim() != branch {
                    let switch = ["switch", "--discard-changes", "-q", branch.as_str()];
                    crate::git::change(
                        &checkout,
                        crate::git::Effect::Metadata,
                        &switch,
                        supervision,
                    )
                    .await?;
                    crate::git::change(
                        &checkout,
                        crate::git::Effect::Checkout,
                        &["clean", "-q", "-ffd"],
                        supervision,
                    )
                    .await?;
                }
                Ok((checkout, lease))
            }
            Taken::Nothing { listed } => {
                let repo = self.repo.clone();
                self.add(listed, &repo, Fresh::On { branch: &branch }, supervision)
                    .await
            }
        }
    }

    /// The checkout run `run_id` works in on this wake: `named`, the one
    /// its log names, while it is still the run's, or else the one the
    /// pool finds for the run's branch — given back while the run was
    /// parked, or taken away.
    pub async fn wake_checkout(
        self: &Arc<Self>,
        run_id: &yunta_core::RunId,
        named: &Path,
        supervision: Supervision<'_>,
    ) -> Result<PathBuf, WorktreeError> {
        if super::release::still_the_runs(named, run_id, supervision).await? {
            return Ok(named.to_path_buf());
        }
        let (checkout, _held) = self.reopen_run(run_id, supervision).await?;
        Ok(checkout)
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
