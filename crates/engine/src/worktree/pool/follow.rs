//! A run following a build into another checkout of its pool. What a
//! unit builds is right only at the path that built it, so once the run's
//! tree stands where a free checkout built last, the run takes that
//! checkout on its branch instead of building the same commit again: its
//! tree goes back to the pool, its own build with it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use yunta_core::CommitSha;

use super::survey::{self, Standing};
use super::{slot, space, CheckoutPool, Lease};
use crate::lock::Contention;
use crate::process::Supervision;
use crate::worktree::WorktreeError;

/// A free checkout standing where the run's tree does, held.
struct Built {
    number: u32,
    path: PathBuf,
    _lease: Lease,
}

impl CheckoutPool {
    /// Moves run `run_id` out of `tree`, the checkout it works in, into the
    /// free checkout of the pool standing exactly where `tree` stands that
    /// was given back last — the one its last unit built in, as that
    /// unit's work landed — when that checkout holds every kind of build
    /// `tree` holds. Its build is of the commit the run stands at, where
    /// `tree`'s is of whatever the tree last built. `tree` lets go of the
    /// run's branch for that checkout to take it, and goes back to the pool
    /// with its build.
    ///
    /// Answers where the run works now; `None` when it stays: `tree` is
    /// not clean on the run's branch, or no free checkout built there. A
    /// move git refused half way is undone, and the run stays.
    pub async fn follow_build(
        &self,
        run_id: &yunta_core::RunId,
        tree: &Path,
        supervision: Supervision<'_>,
    ) -> Result<Option<PathBuf>, WorktreeError> {
        let home = self.home(supervision).await?;
        let Some((own, tree)) = slot::number_of(&home, tree).await else {
            return Ok(None);
        };
        let held = slot::hold(&home, own, Contention::Refuse, None, supervision).await?;
        let Some(tree_lease) = held else {
            return Ok(None);
        };
        let branch = crate::worktree::run_branch(run_id);
        let Some(at) = on_branch(&tree, &branch, supervision).await? else {
            return Ok(None);
        };
        let Some(built) = self.last_built_at(&home, own, &at, supervision).await? else {
            return Ok(None);
        };
        if !holds_every_kind(&tree, &built.path, supervision).await? {
            return Ok(None);
        }
        crate::git::change(
            &tree,
            crate::git::Effect::Metadata,
            &["switch", "--detach", "-q"],
            supervision,
        )
        .await?;
        let take = ["switch", "-q", branch.as_str()];
        if let Err(error) = crate::git::change(
            &built.path,
            crate::git::Effect::Metadata,
            &take,
            supervision,
        )
        .await
        {
            crate::git::change(&tree, crate::git::Effect::Metadata, &take, supervision).await?;
            tracing::warn!(%error, checkout = %built.path.display(), "the run stays in its tree");
            return Ok(None);
        }
        let now = supervision.clock.now();
        slot::used(&home, built.number, self.holder.as_deref(), now).await;
        self.give_back(tree_lease, supervision).await?;
        Ok(Some(built.path))
    }

    /// The free checkout of the pool at `home` standing at `at` that was
    /// given back last with a build, held — any but `own`, the run's tree.
    async fn last_built_at(
        &self,
        home: &Path,
        own: u32,
        at: &CommitSha,
        supervision: Supervision<'_>,
    ) -> Result<Option<Built>, WorktreeError> {
        let detached_at = Standing {
            head: Some(at.clone()),
            branch: None,
        };
        let mut built: Vec<survey::Slot> = survey::survey(home, &self.repo, supervision)
            .await?
            .into_iter()
            .filter(|slot| {
                let kept = slot.held.as_ref().is_some_and(|held| held.unit.is_some());
                slot.number != own
                    && slot.standing.as_ref() == Some(&detached_at)
                    && !kept
                    && slot.built.as_ref().is_some_and(space::Built::holds)
            })
            .collect();
        built.sort_by_key(|slot| std::cmp::Reverse(slot.built.as_ref().map(|built| built.at)));
        for slot in built {
            let held = slot::hold(home, slot.number, Contention::Refuse, None, supervision);
            let Some(lease) = held.await? else {
                continue;
            };
            let free = slot::last_held(home, slot.number)
                .await
                .is_none_or(|held| held.unit.is_none());
            let status = crate::git::tree_state(
                &slot.path,
                &[crate::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE],
                supervision,
            )
            .await?;
            let still = status.clean
                && Standing {
                    head: status.head,
                    branch: status.branch,
                } == detached_at;
            if free && still {
                return Ok(Some(Built {
                    number: slot.number,
                    path: slot.path,
                    _lease: lease,
                }));
            }
        }
        Ok(None)
    }
}

/// Where `tree` stands when it is clean on `branch`.
async fn on_branch(
    tree: &Path,
    branch: &str,
    supervision: Supervision<'_>,
) -> Result<Option<CommitSha>, WorktreeError> {
    let status = crate::git::tree_state(
        tree,
        &[crate::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE],
        supervision,
    )
    .await?;
    let on = status.clean && status.branch.as_deref() == Some(branch);
    Ok(status.head.filter(|_| on))
}

/// Whether `built` holds every kind of build `tree` holds — everything git
/// ignores in `tree`, by its top-level name — and git can say so of both.
async fn holds_every_kind(
    tree: &Path,
    built: &Path,
    supervision: Supervision<'_>,
) -> Result<bool, WorktreeError> {
    let ours = space::ignored(tree, supervision).await?;
    let theirs = space::ignored(built, supervision).await?;
    let theirs: HashSet<&Path> = names(&theirs, built).collect();
    let every = names(&ours, tree).all(|name| theirs.contains(name));
    Ok(every)
}

/// `roots` as named inside `checkout`.
fn names<'a>(roots: &'a [PathBuf], checkout: &'a Path) -> impl Iterator<Item = &'a Path> {
    roots
        .iter()
        .filter_map(move |root| root.strip_prefix(checkout).ok())
}
