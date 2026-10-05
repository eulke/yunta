//! What `gc` does to the pool: what a checkout's build takes is measured
//! where nobody did, a build nobody used within retention is emptied, the
//! builds are kept within the budget, and a free checkout holding no
//! build goes — its source is a `git worktree add` away. What the pool
//! keeps is said in one line.

use std::path::{Path, PathBuf};

use super::room::{Builds, Limits};
use super::{slot, space, survey, CheckoutPool};
use crate::lock::Contention;
use crate::process::Supervision;
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
    /// What their builds take together.
    pub builds: u64,
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
            "{}, builds {} of a {} budget; {} free, {} kept free",
            yunta_core::text::counted(self.checkouts, "checkout"),
            Bytes(self.builds),
            Bytes(self.budget),
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
            self.remove(&slot.path, supervision).await?;
            removed.push(slot.path);
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
    /// with what each took.
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
            let bytes = match &slot.built {
                Some(space::Built {
                    bytes: Some(bytes), ..
                }) => *bytes,
                Some(pending) => {
                    let roots = pending.roots.clone();
                    space::measure_now(home, slot.number, roots, pending.at).await
                }
                None => match space::ignored(&slot.path, supervision).await {
                    Some(roots) => space::measure_now(home, slot.number, roots, now).await,
                    None => continue,
                },
            };
            let used = slot.held.as_ref().map(|held| held.at);
            let idle = unused.is_some_and(|unused| used.is_none_or(|used| now - used > unused));
            if bytes > 0 && idle {
                crate::git::output(&slot.path, &["clean", "-q", "-ffdX"], supervision).await?;
                space::emptied(home, slot.number, now).await;
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
            if survey::look(&slot.path, supervision)
                .await
                .is_some_and(|look| look.clean)
            {
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

    /// Takes the checkout at `path` away from the project.
    async fn remove(&self, path: &Path, supervision: Supervision<'_>) -> Result<(), WorktreeError> {
        let common_dir = &self.located(supervision).await?.common_dir;
        let _mutation_lock =
            crate::worktree::lock_worktree_mutations(common_dir, supervision.clock).await?;
        let shown = path.display().to_string();
        crate::worktree::run_git(
            &self.repo,
            &["worktree", "remove", "--force", &shown],
            supervision,
        )
        .await?;
        Ok(())
    }
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
    let limits = Limits::of(limits, space, largest);
    Ok(PoolReport {
        checkouts: slots.iter().filter(|slot| slot.standing.is_some()).count(),
        builds,
        budget: limits.budget,
        available: space.available,
        keep_free: limits.keep_free,
    })
}
