//! What `gc` does to the pool: what a checkout's build takes is measured
//! where nobody did, a build nobody used within retention is emptied, the
//! builds are kept within the budget, and a free checkout holding no
//! build goes — its source is a `git worktree add` away. What the pool
//! keeps is said in one line.

use std::path::{Path, PathBuf};

use super::room::{Builds, Limits};
use super::{slot, space, survey, CheckoutPool};
use crate::git::{self, Effect};
use crate::lock::Contention;
use crate::process::Supervision;
use crate::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE;
use crate::worktree::WorktreeError;

/// What tidying the pool did, and what it keeps after.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tidied {
    /// The checkouts whose build was emptied, with what each took.
    pub emptied: Vec<(PathBuf, u64)>,
    /// The checkouts that went.
    pub removed: Vec<PathBuf>,
    pub kept: PoolReport,
}

/// What the pool keeps of the disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolReport {
    pub checkouts: usize,
    /// What their counted builds take together.
    pub builds: u64,
    /// Builds nobody counted: they add nothing to `builds`, and are said apart.
    pub uncounted: usize,
    pub budget: u64,
    /// What the disk has free.
    pub available: u64,
    /// What of it always stays free.
    pub keep_free: u64,
}

impl std::fmt::Display for PoolReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use yunta_core::units::Bytes;
        write!(
            f,
            "{}, builds {} of a {} budget",
            yunta_core::text::counted(self.checkouts, "checkout"),
            Bytes(self.builds),
            Bytes(self.budget)
        )?;
        if self.uncounted > 0 {
            write!(
                f,
                " and {} not counted",
                yunta_core::text::counted(self.uncounted, "build")
            )?;
        }
        write!(
            f,
            "; {} free, {} kept free",
            Bytes(self.available),
            Bytes(self.keep_free)
        )
    }
}

impl CheckoutPool {
    /// What the pool keeps of the disk now.
    pub async fn report(&self, supervision: Supervision<'_>) -> Result<PoolReport, WorktreeError> {
        let home = self.home(supervision).await?;
        let slots = survey::survey(&home, &self.repo, supervision).await?;
        report(&self.limits, &home, &slots)
    }

    /// Tidies the pool, `gc`'s part of it: measures every free checkout
    /// nobody measured, empties the builds of free checkouts unused for
    /// longer than `unused`, keeps the builds within the budget, and
    /// removes the free checkouts that hold no build.
    pub async fn tidy(
        &self,
        unused: Option<chrono::Duration>,
        supervision: Supervision<'_>,
    ) -> Result<Tidied, WorktreeError> {
        let home = self.home(supervision).await?;
        let emptied = self.empty_idle(&home, unused, supervision).await?;
        let slots = survey::survey(&home, &self.repo, supervision).await?;
        self.keep_within(Builds::of(&slots), 0, None, supervision)
            .await?;
        let mut removed = Vec::new();
        for slot in self.free(&home, supervision).await? {
            if slot.built.as_ref().is_some_and(space::Built::holds) {
                continue;
            }
            let Some(_lease) = self.upkeep_hold(&home, slot.number, supervision).await? else {
                continue;
            };
            if self
                .remove(&home, slot.number, &slot.path, supervision)
                .await?
            {
                removed.push(slot.path);
            }
        }
        let slots = survey::survey(&home, &self.repo, supervision).await?;
        let kept = report(&self.limits, &home, &slots)?;
        Ok(Tidied {
            emptied,
            removed,
            kept,
        })
    }

    /// Counts the build of every free checkout nobody counted, and empties
    /// those unused for longer than `unused`. Answers the ones emptied,
    /// with what each took. A build that cannot be counted is left as it is.
    async fn empty_idle(
        &self,
        home: &Path,
        unused: Option<chrono::Duration>,
        supervision: Supervision<'_>,
    ) -> Result<Vec<(PathBuf, u64)>, WorktreeError> {
        let now = supervision.clock.now();
        let mut emptied = Vec::new();
        for slot in self.free(home, supervision).await? {
            let Some(_lease) = self.upkeep_hold(home, slot.number, supervision).await? else {
                continue;
            };
            if !still_free(home, slot.number, &slot.path, supervision).await? {
                continue;
            }
            let measured = match &slot.built {
                Some(space::Built {
                    bytes: Some(bytes), ..
                }) => Some(*bytes),
                Some(pending) => {
                    space::measure_now(home, slot.number, pending.roots.clone(), pending.at).await
                }
                None => {
                    let roots = space::ignored(&slot.path, supervision).await?;
                    space::measure_now(home, slot.number, roots, now).await
                }
            };
            let Some(bytes) = measured else {
                continue;
            };
            let used = slot.held.as_ref().map(|held| held.at);
            let idle = unused.is_some_and(|unused| used.is_none_or(|used| now - used > unused));
            if bytes > 0 && idle {
                git::change(
                    &slot.path,
                    Effect::Checkout,
                    &["clean", "-q", "-ffdX"],
                    supervision,
                )
                .await?;
                space::emptied(home, slot.number, now);
                emptied.push((slot.path.clone(), bytes));
            }
        }
        Ok(emptied)
    }

    /// The checkouts of the pool nobody keeps and nothing is uncommitted
    /// in: on no branch, kept for no unit, clean.
    async fn free(
        &self,
        home: &Path,
        supervision: Supervision<'_>,
    ) -> Result<Vec<survey::Slot>, WorktreeError> {
        let mut free = Vec::new();
        for slot in survey::survey(home, &self.repo, supervision).await? {
            let kept = slot
                .held
                .as_ref()
                .and_then(|held| held.unit.as_ref())
                .is_some();
            let detached = slot
                .standing
                .as_ref()
                .is_some_and(|standing| standing.branch.is_none());
            if kept || !detached {
                continue;
            }
            let status =
                git::tree_state(&slot.path, &[SCOPE_EXPANSION_REQUEST_FILE], supervision).await?;
            if status.clean {
                free.push(slot);
            }
        }
        Ok(free)
    }

    async fn upkeep_hold(
        &self,
        home: &Path,
        number: u32,
        supervision: Supervision<'_>,
    ) -> Result<Option<super::Lease>, WorktreeError> {
        slot::hold(home, number, Contention::Refuse, None, supervision).await
    }

    /// Takes the checkout at `path`, number `number` of the pool at `home`,
    /// away from the project, when it is still free. The caller holds the
    /// checkout; the mutation lock taken here covers both the check and the
    /// removal, so nothing lands in the metadata between them. Answers
    /// whether it went.
    async fn remove(
        &self,
        home: &Path,
        number: u32,
        path: &Path,
        supervision: Supervision<'_>,
    ) -> Result<bool, WorktreeError> {
        let common_dir = &self.located(supervision).await?.common_dir;
        let held = git::mutation_lock(common_dir, supervision)
            .await
            .map_err(WorktreeError::Lock)?;
        if !still_free(home, number, path, supervision).await? {
            return Ok(false);
        }
        let shown = path.display().to_string();
        git::change_held(
            &held,
            &self.repo,
            &["worktree", "remove", "--force", &shown],
            supervision,
        )
        .await?;
        Ok(true)
    }
}

/// Whether the checkout at `path`, number `number` of the pool at `home`,
/// is still free: nothing uncommitted in it, and no unit keeps it. Asked
/// under the hold of the checkout, so nothing changes it in between.
async fn still_free(
    home: &Path,
    number: u32,
    path: &Path,
    supervision: Supervision<'_>,
) -> Result<bool, WorktreeError> {
    let kept = slot::last_held(home, number)
        .await
        .and_then(|held| held.unit)
        .is_some();
    if kept {
        return Ok(false);
    }
    let status = git::tree_state(path, &[SCOPE_EXPANSION_REQUEST_FILE], supervision).await?;
    Ok(status.clean)
}

fn report(
    limits: &yunta_core::PoolConfig,
    home: &Path,
    slots: &[survey::Slot],
) -> Result<PoolReport, WorktreeError> {
    let space = yunta_core::disk::space(home).map_err(|source| WorktreeError::Io {
        action: "read how much of the disk is free".to_string(),
        path: home.to_path_buf(),
        source,
    })?;
    let counted = || {
        slots
            .iter()
            .filter_map(|slot| slot.built.as_ref().map(space::Built::counted))
    };
    let builds: u64 = counted().sum();
    let largest = counted().max().unwrap_or(0);
    let uncounted = slots
        .iter()
        .filter_map(|slot| slot.built.as_ref())
        .filter(|built| built.size() == space::Size::Uncounted)
        .count();
    let limits = Limits::of(limits, space, largest);
    Ok(PoolReport {
        checkouts: slots.iter().filter(|slot| slot.standing.is_some()).count(),
        builds,
        uncounted,
        budget: limits.budget,
        available: space.available,
        keep_free: limits.keep_free,
    })
}
