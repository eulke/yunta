//! Which checkout of a survey fits its next user best, read from what the
//! survey already holds: whose the checkout is to take, whether it holds
//! a build, and how recently and how near to where the user starts it
//! was left. Nothing here asks git.

use std::cmp::Reverse;

use yunta_core::CommitSha;

use super::survey::{Slot, Standing};

/// Who takes a checkout of the pool.
#[derive(Debug, Clone, Copy)]
pub(super) enum User<'a> {
    /// A run, which works on its own branch.
    Run { branch: &'a str },
    /// A unit, whose attempts' refs all start with `refs`: it works on no
    /// branch, and the checkout's record says it is the unit's.
    Unit { refs: &'a str },
    /// A probe or a measurement, which owns nothing.
    Passing,
}

/// Whose a checkout is, for its next user. Only a clean checkout is ever
/// handed out: what is uncommitted in one is somebody's, which the look
/// taken once it is held tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Claim {
    /// The user's own — on its branch, or kept for it: it goes back to
    /// its owner first.
    Own,
    /// Anybody's: on no branch, kept for nobody.
    Free,
    /// Kept for another unit, whose work it holds: anybody's once that
    /// work is in where the user starts — kept for its unit until then,
    /// so the unit can still pick its session back up there. Only git can
    /// say whether it landed, so it is asked last.
    Landed,
}

/// Whose a checkout standing at `standing`, kept for `unit` as its record
/// says, is for `user` — `None` when it is not the user's to take: on a
/// run's branch, which is that run's for as long as it lives, or on any
/// other branch, a person's.
pub(super) fn claim(standing: &Standing, unit: Option<&str>, user: User<'_>) -> Option<Claim> {
    match (standing.branch.as_deref(), unit, user) {
        (Some(on), _, User::Run { branch }) if on == branch => Some(Claim::Own),
        (Some(on), _, _) if crate::worktree::is_legacy_unit_branch(on) => Some(Claim::Landed),
        (Some(_), _, _) => None,
        (None, Some(kept), User::Unit { refs }) if kept.starts_with(refs) => Some(Claim::Own),
        (None, Some(_), _) => Some(Claim::Landed),
        (None, None, _) => Some(Claim::Free),
    }
}

/// What is known of a checkout's build, best first: one that holds a
/// build builds again only what changed, where an empty one builds
/// everything, its dependencies first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Build {
    Held,
    /// Never measured: a checkout given back before builds were recorded.
    Unknown,
    Empty,
}

/// One checkout a user may take, and what ranks it.
pub(super) struct Candidate {
    pub(super) slot: Slot,
    pub(super) claim: Claim,
    build: Build,
    at_target: bool,
}

impl Candidate {
    pub(super) fn at_target(&self) -> bool {
        self.at_target
    }

    /// Whether what it holds is known to be no build: taking it builds
    /// everything there.
    pub(super) fn is_empty(&self) -> bool {
        self.build == Build::Empty
    }

    /// How it ranks when `away` paths separate it from where its user
    /// starts: lower is better.
    pub(super) fn rank(
        &self,
        away: usize,
    ) -> (
        Claim,
        Build,
        usize,
        Reverse<Option<chrono::DateTime<chrono::Utc>>>,
        u32,
    ) {
        let used = self.slot.held.as_ref().map(|held| held.at);
        (
            self.claim,
            self.build,
            away,
            Reverse(used),
            self.slot.number,
        )
    }
}

/// The checkouts of a survey `user` may take to start from `target`,
/// best first, with what it takes to tell how far each is left aside:
/// one already at `target` counts as no distance, any other as one, so
/// the most recently used at a commit of its own comes after every one at
/// `target` with the same build.
pub(super) fn ranked(slots: Vec<Slot>, user: User<'_>, target: &CommitSha) -> Vec<Candidate> {
    let mut candidates: Vec<Candidate> = slots
        .into_iter()
        .filter_map(|slot| {
            let standing = slot.standing.as_ref()?;
            let unit = slot.held.as_ref().and_then(|held| held.unit.as_deref());
            let claim = claim(standing, unit, user)?;
            let at_target = standing.head.as_ref() == Some(target);
            let build = match &slot.built {
                Some(built) if built.holds() => Build::Held,
                Some(_) => Build::Empty,
                None => Build::Unknown,
            };
            Some(Candidate {
                slot,
                claim,
                build,
                at_target,
            })
        })
        .collect();
    candidates.sort_by_key(|candidate| candidate.rank(usize::from(!candidate.at_target)));
    candidates
}
