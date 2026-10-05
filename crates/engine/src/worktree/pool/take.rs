//! Taking a checkout of the pool for a user: the most fitting one, from
//! one survey of the pool, held and looked at again before it is handed
//! out.

use std::path::Path;

use yunta_core::CommitSha;

use super::{fit, room, slot, space, survey, CheckoutPool, Taken};
use crate::lock::Contention;
use crate::process::Supervision;
use crate::worktree::WorktreeError;

impl CheckoutPool {
    /// The most fitting checkout for `user` to start from `target`, held
    /// for it and kept for nobody until its user says otherwise: ranked
    /// from one survey of the pool, the best few told apart by how many
    /// paths separate each from `target`, and looked at once held —
    /// another process may have changed it since the survey.
    pub(super) async fn take(
        &self,
        user: fit::User<'_>,
        target: &CommitSha,
        supervision: Supervision<'_>,
    ) -> Result<Taken, WorktreeError> {
        let home = self.home(supervision).await?;
        let slots = survey::survey(&home, &self.repo, supervision).await?;
        let listed = slots
            .iter()
            .filter(|slot| slot.standing.is_some())
            .map(|slot| slot.number)
            .collect();
        space::count_pending(&home, &slots, &self.walking);
        let builds = room::Builds::of(&slots);
        let ranked = fit::ranked(slots, user, target);
        self.room_for(ranked.first(), builds, supervision).await?;
        let ordered = nearest_first(ranked, target, supervision).await;
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
            let (number, path) = (candidate.slot.number, candidate.slot.path);
            if still_fits(&home, number, &path, user, target, supervision).await {
                slot::owned(&home, number, None).await;
                return Ok(Taken::Held(path, lease));
            }
        }
        Ok(Taken::Nothing { listed })
    }
}

/// `ranked` with its best few told apart by how many paths separate each
/// from `target` — what the survey could not tell; with one, there is
/// nothing to tell apart.
async fn nearest_first(
    mut ranked: Vec<fit::Candidate>,
    target: &CommitSha,
    supervision: Supervision<'_>,
) -> impl Iterator<Item = fit::Candidate> {
    let best = match ranked.len() {
        0 | 1 => 0,
        many => many.min(3),
    };
    let mut nearest = Vec::with_capacity(best);
    for candidate in ranked.drain(..best) {
        let away = match candidate.at_target() {
            true => 0,
            false => distance(&candidate.slot.path, target, supervision).await,
        };
        nearest.push((candidate, away));
    }
    nearest.sort_by_key(|(candidate, away)| candidate.rank(*away));
    nearest
        .into_iter()
        .map(|(candidate, _)| candidate)
        .chain(ranked)
}

/// Whether `checkout` — checkout `number` of the pool at `home`, held —
/// is still one `user` may take: clean, and its user's own, free, or
/// kept for another unit whose work is in `target`, which only here is
/// asked of git.
async fn still_fits(
    home: &Path,
    number: u32,
    checkout: &Path,
    user: fit::User<'_>,
    target: &CommitSha,
    supervision: Supervision<'_>,
) -> bool {
    let Some(look) = survey::look(checkout, supervision).await else {
        return false;
    };
    if !look.clean {
        return false;
    }
    let unit = slot::last_held(home, number)
        .await
        .and_then(|held| held.unit);
    match fit::claim(&look.standing, unit.as_deref(), user) {
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
