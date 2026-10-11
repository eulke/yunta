//! What the pool may keep of the disk: its builds within a budget, and a
//! floor of the disk always free. A build is the weight of a checkout —
//! gigabytes where its source is megabytes — so making room is emptying
//! builds: what git ignores in a free checkout, the least recently used
//! first. The source stays, and so does anybody's work.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use yunta_core::disk::Space;
use yunta_core::PoolConfig;

use super::survey::Slot;
use super::{slot, space, CheckoutPool};
use crate::lock::Contention;
use crate::process::Supervision;
use crate::worktree::{Room, WorktreeError};

/// The pool's limits on a file system of `space`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Limits {
    /// What the builds of the pool may take together.
    pub(super) budget: u64,
    /// What of the disk always stays free.
    pub(super) keep_free: u64,
}

impl Limits {
    /// What `config` says, and what a file system of `space` and builds
    /// of up to `largest` bytes make of what it leaves unsaid: a fifth of
    /// the disk for builds, and no more than three of the largest — a
    /// task's, a measurement's and the run's own, each warm; more than
    /// that would stand idle — and the larger of a twentieth of the disk
    /// and 10 GB always free.
    pub(super) fn of(config: &PoolConfig, space: Space, largest: u64) -> Limits {
        let budget = config.budget.map_or_else(
            || {
                let share = space.total / 100 * 20;
                match largest {
                    0 => share,
                    largest => share.min(largest.saturating_mul(3)),
                }
            },
            |budget| budget.of(space.total),
        );
        let keep_free = config.keep_free.map_or_else(
            || (space.total / 100 * 5).max(10_000_000_000),
            |keep_free| keep_free.of(space.total),
        );
        Limits { budget, keep_free }
    }
}

/// What the builds of a survey of the pool come to.
pub(super) struct Builds {
    /// Every build the pool measured, together.
    held: u64,
    /// The largest of them: what one more cold build takes, near enough.
    pub(super) largest: u64,
    /// The builds the pool may empty, the least recently used first.
    emptiable: Vec<Emptiable>,
}

/// A build in a checkout nobody keeps: on no branch, kept for no unit.
struct Emptiable {
    number: u32,
    path: PathBuf,
    bytes: u64,
    used: Option<DateTime<Utc>>,
}

impl Builds {
    pub(super) fn of(slots: &[Slot]) -> Builds {
        let measured = || {
            slots
                .iter()
                .filter_map(|slot| slot.built.as_ref().map(super::space::Built::counted))
        };
        let mut emptiable: Vec<Emptiable> = slots.iter().filter_map(emptiable).collect();
        emptiable.sort_by_key(|build| build.used);
        Builds {
            held: measured().sum(),
            largest: measured().max().unwrap_or(0),
            emptiable,
        }
    }
}

fn emptiable(slot: &Slot) -> Option<Emptiable> {
    let built = slot.built.as_ref()?;
    let bytes = built.counted();
    let standing = slot.standing.as_ref()?;
    let held = slot.held.as_ref();
    let kept = held.and_then(|held| held.unit.as_ref()).is_some();
    (built.holds() && standing.branch.is_none() && !kept).then(|| Emptiable {
        number: slot.number,
        path: slot.path.clone(),
        bytes,
        used: held.map(|held| held.at),
    })
}

impl CheckoutPool {
    /// `slots` ranked for `user` to start from `target`, with room made for
    /// what it builds. For a user that builds, the checkout it is about to
    /// take is spared, and room is made for a whole build in one that holds
    /// none. A user that builds nothing needs no room, and takes the
    /// emptiest checkout: the budget is kept first, so a build it empties
    /// is one the user may take.
    pub(super) async fn ranked_with_room(
        &self,
        mut slots: Vec<Slot>,
        user: super::fit::User<'_>,
        target: &yunta_core::CommitSha,
        supervision: Supervision<'_>,
    ) -> Result<Vec<super::fit::Candidate>, WorktreeError> {
        let builds = Builds::of(&slots);
        if !user.builds() {
            let emptied = self.keep_within(builds, 0, None, supervision).await?;
            let now = supervision.clock.now();
            for slot in slots
                .iter_mut()
                .filter(|slot| emptied.contains(&slot.number))
            {
                slot.built = Some(space::Built::empty(now));
            }
            return Ok(super::fit::ranked(slots, user, target));
        }
        let ranked = super::fit::ranked(slots, user, target);
        let best = ranked.first();
        let need = match best {
            Some(best) if !best.is_empty() => 0,
            _ => builds.largest,
        };
        let spare = best.map(|best| best.slot.number);
        self.keep_within(builds, need, spare, supervision).await?;
        Ok(ranked)
    }

    /// Makes room for `need` more bytes: keeps the pool's builds within
    /// its budget, and `need` within what the disk has above its floor,
    /// by emptying builds of free checkouts — never `spare`'s, the
    /// checkout about to be taken, and never a held one's. Fails with
    /// [`WorktreeError::NoRoom`] when that still leaves too little.
    /// Answers the checkouts whose build it emptied.
    pub(super) async fn keep_within(
        &self,
        builds: Builds,
        need: u64,
        spare: Option<u32>,
        supervision: Supervision<'_>,
    ) -> Result<Vec<u32>, WorktreeError> {
        let home = self.home(supervision).await?;
        let space = space_of(&home)?;
        let limits = Limits::of(&self.limits, space, builds.largest);
        let mut held = builds.held;
        let mut available = space.available;
        let short = |available: u64| available < need.saturating_add(limits.keep_free);
        let mut emptied = Vec::new();
        for build in builds.emptiable {
            if held <= limits.budget && !short(available) {
                break;
            }
            if Some(build.number) == spare || !self.empty(&home, &build, supervision).await? {
                continue;
            }
            emptied.push(build.number);
            held = held.saturating_sub(build.bytes);
            available = space_of(&home).map_or(available, |space| space.available);
        }
        match short(available) {
            true => Err(WorktreeError::NoRoom {
                room: Room {
                    available,
                    need,
                    keep_free: limits.keep_free,
                },
            }),
            false => Ok(emptied),
        }
    }

    /// Empties `build` — what git ignores in its checkout, held for the
    /// moment — and records it empty. Answers whether it could: a
    /// checkout somebody holds is left as it is.
    async fn empty(
        &self,
        home: &Path,
        build: &Emptiable,
        supervision: Supervision<'_>,
    ) -> Result<bool, WorktreeError> {
        let held = slot::hold(home, build.number, Contention::Refuse, None, supervision);
        let Some(_lease) = held.await? else {
            return Ok(false);
        };
        crate::git::output(&build.path, &["clean", "-q", "-ffdX"], supervision).await?;
        space::emptied(home, build.number, supervision.clock.now());
        Ok(true)
    }

    /// Whether the disk keeps its floor free — making room, when it does
    /// not, the way taking a checkout does.
    pub async fn keeps_its_floor(&self, supervision: Supervision<'_>) -> Result<(), WorktreeError> {
        let home = self.home(supervision).await?;
        let space = space_of(&home)?;
        if space.available >= Limits::of(&self.limits, space, 0).keep_free {
            return Ok(());
        }
        let slots = super::survey::survey(&home, &self.repo, supervision).await?;
        self.keep_within(Builds::of(&slots), 0, None, supervision)
            .await
            .map(drop)
    }
}

fn space_of(home: &Path) -> Result<Space, WorktreeError> {
    yunta_core::disk::space(home).map_err(|source| WorktreeError::Io {
        action: "read how much of the disk is free".to_string(),
        path: home.to_path_buf(),
        source,
    })
}
