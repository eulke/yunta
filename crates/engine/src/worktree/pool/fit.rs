//! Which checkout of a survey fits its next user best, read from what the
//! survey already holds: whose the checkout is to take, whether it holds
//! a build, and how recently and how near to where the user starts it
//! was left. Nothing here asks git.

use std::cmp::Reverse;

use yunta_core::CommitSha;

use super::survey::{Slot, Standing};

/// Whose a checkout is, for a user owning the branches starting with
/// `owner` (none, for a user that works on no branch). Only a clean
/// checkout is ever handed out: what is uncommitted in one is somebody's,
/// which the look taken once it is held tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Claim {
    /// On one of the user's own branches: it goes back to its owner
    /// first.
    Own,
    /// On no branch: anybody's.
    Free,
    /// On another unit's branch: anybody's once that unit's work is in
    /// where the user starts — kept on its branch until somebody takes it,
    /// so its owner can still pick its session back up there. Only git
    /// can say whether it landed, so it is asked last.
    Landed,
}

/// Whose `standing` makes a checkout — `None` when it is not the user's
/// to take: on a run's branch, which is that run's for as long as it
/// lives, or on any other branch, a person's.
pub(super) fn claim(standing: &Standing, owner: Option<&str>) -> Option<Claim> {
    match standing.branch.as_deref() {
        None => Some(Claim::Free),
        Some(on) if owner.is_some_and(|owner| on.starts_with(owner)) => Some(Claim::Own),
        Some(on) if crate::worktree::is_unit_branch(on) => Some(Claim::Landed),
        Some(_) => None,
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

/// The checkouts of a survey `owner` may take to start from `target`,
/// best first, with what it takes to tell how far each is left aside:
/// one already at `target` counts as no distance, any other as one, so
/// the most recently used at a commit of its own comes after every one at
/// `target` with the same build.
pub(super) fn ranked(slots: Vec<Slot>, owner: Option<&str>, target: &CommitSha) -> Vec<Candidate> {
    let mut candidates: Vec<Candidate> = slots
        .into_iter()
        .filter_map(|slot| {
            let standing = slot.standing.as_ref()?;
            let claim = claim(standing, owner)?;
            let at_target = standing.head.as_ref() == Some(target);
            let build = match slot.built {
                Some(built) if built.bytes > 0 => Build::Held,
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
