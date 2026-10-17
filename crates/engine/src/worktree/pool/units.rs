//! A unit's checkout in the pool: taken back by a later attempt that
//! picks its session up, let go once the unit's work landed, and freed
//! when its run ends. A unit works on no branch: its ref keeps what it
//! commits, and the checkout's record says the checkout is the unit's.

use std::cmp::Reverse;
use std::path::Path;
use std::sync::Arc;

use yunta_core::{CommitSha, RunId};

use super::{slot, survey, CheckoutPool, Lease};
use crate::git::{self, Effect};
use crate::lock::Contention;
use crate::process::Supervision;
use crate::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE;
use crate::worktree::{head_commit, head_tree, Unit, UnitId, UnitRef, WorktreeError};

impl CheckoutPool {
    /// Reopens the checkout a session saw, for a session picked back up,
    /// held for it: the one kept for `who` in `run_id` that holds `left` —
    /// the work a blocked task's attempt committed — or, with no `left`,
    /// the one its latest attempt was kept in, which left its work as it
    /// was. The work goes back to where the attempt had it: uncommitted
    /// changes on the tree the unit began from, which is what its audit
    /// answers for.
    ///
    /// Found by the pool's records, not by an attempt number: a session
    /// picked back up works on in the unit it saw, so an attempt does not
    /// always open a unit of its own. `None` when no such checkout is
    /// still there.
    pub async fn reopen_unit(
        self: &Arc<Self>,
        run_id: &RunId,
        who: UnitId,
        left: Option<&CommitSha>,
        supervision: Supervision<'_>,
    ) -> Result<Option<(Unit, Lease)>, WorktreeError> {
        let home = self.home(supervision).await?;
        let refs = UnitRef::of_unit(run_id, &who);
        let mut kept: Vec<(UnitRef, survey::Slot)> = survey::survey(&home, &self.repo, supervision)
            .await?
            .into_iter()
            .filter_map(|slot| {
                let unit = UnitRef::named(slot.held.as_ref()?.unit.as_deref()?)?;
                unit.name().starts_with(&refs).then_some((unit, slot))
            })
            .collect();
        kept.sort_by_key(|(unit, _)| Reverse(unit.attempt()));
        for (keeps, slot) in kept {
            let lease = self.hold_for_its_unit(&home, slot.number, &slot.path, supervision);
            let lease = lease.await?;
            let Ok(head) = head_commit(&slot.path, supervision).await else {
                if left.is_none() {
                    return Ok(None);
                }
                continue;
            };
            let base = match left {
                None => head,
                Some(left) if head == *left => {
                    crate::worktree::unit::back_to_base(&slot.path, left, supervision).await?
                }
                Some(_) => continue,
            };
            let from = head_tree(&slot.path, supervision).await?;
            let unit = Unit {
                who,
                worktree: slot.path,
                base,
                from,
                keeps,
            };
            return Ok(Some((unit, lease)));
        }
        Ok(None)
    }

    /// Holds checkout `number`, at `checkout`, for the unit that reopens
    /// it. Another process may be looking at it for a moment, so this
    /// waits that moment out.
    async fn hold_for_its_unit(
        &self,
        home: &Path,
        number: u32,
        checkout: &Path,
        supervision: Supervision<'_>,
    ) -> Result<Lease, WorktreeError> {
        let moment = Contention::Wait {
            patience: std::time::Duration::from_secs(5),
            poll: std::time::Duration::from_millis(15),
        };
        slot::hold(home, number, moment, self.holder.as_deref(), supervision)
            .await?
            .ok_or_else(|| WorktreeError::CheckoutHeld {
                path: checkout.to_path_buf(),
            })
    }

    /// Lets go of a task's `unit`, its checkout still held by `lease`, once
    /// its work landed: its attempts' refs go (`forget_attempts`) and the
    /// checkout is kept for nobody.
    pub async fn retire(
        &self,
        unit: &Unit,
        lease: &Lease,
        supervision: Supervision<'_>,
    ) -> Result<(), WorktreeError> {
        forget_attempts(unit, supervision).await?;
        lease.keep_for(None).await;
        Ok(())
    }

    /// Frees every checkout kept for a unit of `run_id`, as the run ends:
    /// a clean one is kept for nobody — what its unit committed is kept by
    /// its ref — and one with something uncommitted stays its unit's. The
    /// refs whose work `into`, the run's tree as it closes, holds go; a
    /// blocked task's stays until the run is collected.
    pub(super) async fn free_units_of(
        &self,
        into: &Path,
        run_id: &RunId,
        supervision: Supervision<'_>,
    ) -> Result<(), WorktreeError> {
        let home = self.home(supervision).await?;
        let ours = UnitRef::of_run(run_id);
        for slot in survey::survey(&home, &self.repo, supervision).await? {
            let kept = slot.held.as_ref().and_then(|held| held.unit.as_deref());
            if !kept.is_some_and(|kept| kept.starts_with(&ours)) {
                continue;
            }
            let held = slot::hold(&home, slot.number, Contention::Refuse, None, supervision);
            let Some(lease) = held.await? else {
                continue;
            };
            if git::tree_state(&slot.path, &[SCOPE_EXPANSION_REQUEST_FILE], supervision)
                .await?
                .clean
            {
                lease.keep_for(None).await;
            }
        }
        let landed = head_commit(into, supervision).await?;
        let merged = format!("--merged={}", landed.as_str());
        for unit in refs(into, &ours, Some(&merged), supervision).await? {
            let forget = ["update-ref", "-d", unit.name()];
            git::change(into, Effect::Refs, &forget, supervision).await?;
        }
        Ok(())
    }

    /// Lets go of everything the units of `run_id` kept, landed or not: the
    /// run is being collected, and nothing will read what a blocked task
    /// left. A checkout kept for one of them is kept for nobody, unless
    /// something in it is uncommitted.
    pub(super) async fn forget_units_of(
        &self,
        repo: &Path,
        run_id: &RunId,
        supervision: Supervision<'_>,
    ) -> Result<(), WorktreeError> {
        let ours = UnitRef::of_run(run_id);
        let home = self.home(supervision).await?;
        for slot in survey::survey(&home, &self.repo, supervision).await? {
            let kept = slot.held.as_ref().and_then(|held| held.unit.as_deref());
            if !kept.is_some_and(|kept| kept.starts_with(&ours)) {
                continue;
            }
            let held = slot::hold(&home, slot.number, Contention::Refuse, None, supervision);
            let Some(lease) = held.await? else {
                continue;
            };
            if git::tree_state(&slot.path, &[SCOPE_EXPANSION_REQUEST_FILE], supervision)
                .await?
                .clean
            {
                lease.keep_for(None).await;
            }
        }
        for unit in refs(repo, &ours, None, supervision).await? {
            let forget = ["update-ref", "-d", unit.name()];
            git::change(repo, Effect::Refs, &forget, supervision).await?;
        }
        Ok(())
    }
}

/// Lets go of every ref the attempts of `unit` kept, its work landed: the
/// run's tree holds what the landing attempt did, replayed onto it — a
/// commit of its own, which no ref of the unit names — and what the other
/// attempts left is superseded by it.
pub async fn forget_attempts(
    unit: &Unit,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    let prefix = unit.keeps.every_attempt();
    for kept in refs(&unit.worktree, prefix, None, supervision).await? {
        let forget = ["update-ref", "-d", kept.name()];
        git::change(&unit.worktree, Effect::Refs, &forget, supervision).await?;
    }
    Ok(())
}

/// The units' refs of `repo` under `prefix` — only those `merged` names
/// as already in a commit, when it says so.
async fn refs(
    repo: &Path,
    prefix: &str,
    merged: Option<&str>,
    supervision: Supervision<'_>,
) -> Result<Vec<UnitRef>, WorktreeError> {
    let mut args = vec!["for-each-ref", "--format=%(refname)"];
    args.extend(merged);
    args.push(prefix);
    let listed = git::read(repo, &args, supervision).await?;
    Ok(listed.lines().filter_map(UnitRef::named).collect())
}
