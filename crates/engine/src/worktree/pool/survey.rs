//! The pool as one look shows it: every checkout under its directory,
//! where the repository's list of worktrees says it stands — the commit,
//! the branch — and what its records say of its last use and its build.
//! One listing answers for every checkout, however many the pool holds;
//! a checkout is looked at on its own only once it is held.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use yunta_core::CommitSha;

use super::slot::{self, Held};
use super::space::{self, Built};
use crate::process::Supervision;
use crate::worktree::WorktreeError;

/// One checkout of the pool, as the survey found it.
pub(super) struct Slot {
    pub(super) number: u32,
    pub(super) path: PathBuf,
    /// Where it stands, as the repository lists it — `None` for a
    /// directory git does not list as a checkout: one a crash left half
    /// added, or one gone from git's records.
    pub(super) standing: Option<Standing>,
    pub(super) held: Option<Held>,
    pub(super) built: Option<Built>,
}

/// Where a checkout stands: the commit, and the branch it is on — its
/// short name, `None` on a detached `HEAD`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Standing {
    pub(super) head: Option<CommitSha>,
    pub(super) branch: Option<String>,
}

/// Every checkout of the pool at `home`, the project `repo` is a checkout
/// of: one listing of the repository's worktrees, and each checkout's
/// records read off disk.
pub(super) async fn survey(
    home: &Path,
    repo: &Path,
    supervision: Supervision<'_>,
) -> Result<Vec<Slot>, WorktreeError> {
    let listing = crate::git::read(repo, &["worktree", "list", "--porcelain"], supervision).await?;
    let mut listed = HashMap::new();
    for checkout in crate::worktree::checkouts(&listing) {
        if checkout.prunable {
            continue;
        }
        let path = tokio::fs::canonicalize(&checkout.path)
            .await
            .unwrap_or(checkout.path);
        let branch = checkout.branch.map(|branch| short(&branch).to_string());
        let standing = Standing {
            head: checkout.head,
            branch,
        };
        listed.insert(path, standing);
    }
    let mut slots = Vec::new();
    for (number, path) in slot::listed(home).await {
        slots.push(Slot {
            number,
            standing: listed.remove(&path),
            held: slot::last_held(home, number).await,
            built: space::built(home, number).await,
            path,
        });
    }
    Ok(slots)
}

/// A branch's short name: `refs/heads/` taken off.
fn short(branch: &str) -> &str {
    branch.strip_prefix("refs/heads/").unwrap_or(branch)
}
