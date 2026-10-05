//! Taking a checkout of the pool for a user: the most fitting one, from
//! one survey of the pool, held and looked at again before it is handed
//! out.

use std::path::{Path, PathBuf};

use yunta_core::CommitSha;

use super::{fit, slot, survey, CheckoutPool, Lease};
use crate::lock::Contention;
use crate::process::Supervision;
use crate::worktree::WorktreeError;

impl CheckoutPool {
    /// The most fitting checkout for a user owning the branches starting
    /// with `owner` and starting from `target`, held for it: ranked from
    /// one survey of the pool, the best few told apart by how many paths
    /// separate each from `target`, and looked at once held — another
    /// process may have changed it since the survey.
    pub(super) async fn take(
        &self,
        owner: Option<&str>,
        target: &CommitSha,
        supervision: Supervision<'_>,
    ) -> Result<Option<(PathBuf, Lease)>, WorktreeError> {
        let home = self.home(supervision).await?;
        let slots = survey::survey(&home, &self.repo, supervision).await?;
        let mut ranked = fit::ranked(slots, owner, target);
        let best = ranked.len().min(3);
        let mut nearest = Vec::with_capacity(best);
        for candidate in ranked.drain(..best) {
            let away = match candidate.at_target() {
                true => 0,
                false => distance(&candidate.slot.path, target, supervision).await,
            };
            nearest.push((candidate, away));
        }
        nearest.sort_by_key(|(candidate, away)| candidate.rank(*away));
        let ordered = nearest
            .into_iter()
            .map(|(candidate, _)| candidate)
            .chain(ranked);
        for candidate in ordered {
            let held = slot::hold(
                &home,
                candidate.slot.number,
                Contention::Refuse,
                self.holder.as_deref(),
                supervision,
            );
            let Some(lease) = held.await? else {
                continue;
            };
            let path = candidate.slot.path;
            if still_fits(&path, owner, target, supervision).await {
                return Ok(Some((path, lease)));
            }
        }
        Ok(None)
    }
}

/// Whether `checkout`, held, is still one its user may take: clean, and
/// on a branch the user owns, on none, or on another unit's whose work
/// is in `target` — which only here is asked of git.
async fn still_fits(
    checkout: &Path,
    owner: Option<&str>,
    target: &CommitSha,
    supervision: Supervision<'_>,
) -> bool {
    let Some(look) = survey::look(checkout, supervision).await else {
        return false;
    };
    if !look.clean {
        return false;
    }
    match fit::claim(&look.standing, owner) {
        Some(fit::Claim::Own | fit::Claim::Free) => true,
        Some(fit::Claim::Landed) => crate::git::success(
            checkout,
            &["merge-base", "--is-ancestor", "HEAD", target.as_str()],
            supervision,
        )
        .await
        .unwrap_or(false),
        None => false,
    }
}

/// How many paths differ between what `checkout` holds and `target`: what
/// a build there has to redo.
async fn distance(checkout: &Path, target: &CommitSha, supervision: Supervision<'_>) -> usize {
    crate::git::output(
        checkout,
        &["diff", "--name-only", "HEAD", target.as_str()],
        supervision,
    )
    .await
    .map_or(usize::MAX, |names| names.lines().count())
}
