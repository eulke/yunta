//! One unit of work and the tree it owns while it works.
//!
//! A run's work is done by units: a node of the graph, and a task of a
//! `loop` node. A unit opens a checkout of its own, does its work there
//! answering only for what it changed, and lands that work on the tree
//! it shares with the rest of the run — replayed onto that tree as it
//! stands at the moment of landing, then fast-forwarded onto.
//!
//! Landing is two moments and not one, because something has to happen
//! between them: the replay puts the unit's work on ground that may have
//! moved since it began, and green where it worked is necessary but
//! never sufficient. Whoever owns the unit re-verifies it there — which
//! is its own business, not this module's — and only then does the
//! shared tree move.

use std::path::{Path, PathBuf};

use yunta_core::{CommitSha, Isolation, NodeId, RunId, TaskId, TreeId};

use super::{head_commit, prepare_worktree, WorktreeError};
use crate::process::Supervision;

/// Which unit of work owns a tree, a branch and a private index.
///
/// The kind is part of the name, not decoration: a node and a task of
/// one run may be called the same thing and still run at the same
/// moment, and everything a unit owns is named after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitId {
    /// A node of the graph.
    Node(NodeId),
    /// A task of a `loop` node.
    Task(TaskId),
}

impl std::fmt::Display for UnitId {
    /// How a unit is named wherever a name is needed — a directory, a
    /// branch, the hash of a private index. One rendering, so a unit's
    /// tree and a unit's index cannot disagree about whose they are.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnitId::Node(id) => write!(f, "node/{id}"),
            UnitId::Task(id) => write!(f, "task/{id}"),
        }
    }
}

/// The tree one unit of work owns while it works.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    /// Whose work this is.
    pub who: UnitId,
    /// The checkout it works in, which is nobody else's.
    pub worktree: PathBuf,
    /// The commit its branch was cut from — where its own history
    /// starts, and therefore what a replay must not carry along.
    pub base: CommitSha,
    /// The tree it started from — what its own diff is judged against.
    /// The tree of [`base`](Unit::base), read once when the unit opened,
    /// held as its own value because an audit compares trees and never
    /// asks git again.
    pub from: TreeId,
}

/// Where a run opens its units: the repository their checkouts are
/// linked to, the directory those checkouts go under, the run they
/// belong to and the commit they branch from.
///
/// Identical for every unit of one run, so it travels as one value and
/// only `who` and the attempt vary per unit.
#[derive(Clone, Copy)]
pub struct UnitHome<'a> {
    pub repo: &'a Path,
    pub run_dir: &'a Path,
    pub run_id: &'a RunId,
    pub base: &'a CommitSha,
}

/// A commit naming exactly what `repo` holds this moment: its `HEAD`
/// plus whatever is lying in its working tree uncommitted.
///
/// A unit branches from what it would have found, not from the last
/// thing anybody committed — and between two nodes of a run nobody
/// commits, so those are different trees. Nothing points at this commit
/// but the unit's own branch: `repo`'s branch does not move, and what is
/// uncommitted there stays uncommitted, to be landed over later.
pub async fn snapshot_commit(
    repo: &Path,
    index: &Path,
    supervision: Supervision<'_>,
) -> Result<CommitSha, WorktreeError> {
    let tree = super::capture_tree(repo, index, supervision).await?;
    let head = head_commit(repo, supervision).await?;
    let printed = crate::git::output(
        repo,
        &[
            "commit-tree",
            tree.as_str(),
            "-p",
            head.as_str(),
            "-m",
            "the tree a unit of work started from",
        ],
        supervision,
    )
    .await?;
    printed
        .trim()
        .parse()
        .map_err(|source: yunta_core::InvalidId| WorktreeError::NotACommit {
            args: "commit-tree".to_string(),
            cwd: repo.to_path_buf(),
            source,
        })
}

/// Opens `who`'s own checkout at the run's base commit, on a branch of
/// its own, and records the tree it therefore starts from.
///
/// The attempt number is part of both names: a unit re-dispatched after
/// an orphaned attempt gets a tree of its own rather than whatever the
/// interrupted attempt left in its.
pub async fn open_unit(
    home: UnitHome<'_>,
    who: UnitId,
    attempt: u32,
    supervision: Supervision<'_>,
) -> Result<Unit, WorktreeError> {
    let worktree = crate::run_dir::unit_worktrees(home.run_dir).join(format!("{who}-{attempt}"));
    prepare_worktree(
        home.repo,
        &worktree,
        home.base,
        &super::unit_branch(home.run_id, &who, attempt),
        Isolation::Worktree,
        supervision,
    )
    .await?;
    // The checkout was just made at `base`, so its `HEAD` is exactly
    // what this unit begins with — no capture of the working tree is
    // owed for a tree nobody else has touched.
    let from = super::head_tree(&worktree, supervision).await?;
    Ok(Unit {
        who,
        worktree,
        base: home.base.clone(),
        from,
    })
}

/// Commits everything the unit did, under `message`.
///
/// A unit that changed nothing produces no commit and is not an error:
/// work whose criteria are met by side effects that leave no diff is
/// still work done.
pub async fn commit_work(
    unit: &Unit,
    message: &str,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    crate::git::output(&unit.worktree, &["add", "-A"], supervision).await?;
    // `diff --cached --quiet` exits 0 with nothing staged, 1 with staged
    // changes — both are answers, not failures.
    if crate::git::success(
        &unit.worktree,
        &["diff", "--cached", "--quiet"],
        supervision,
    )
    .await?
    {
        return Ok(());
    }
    crate::git::output(
        &unit.worktree,
        &["commit", "-q", "-m", message],
        supervision,
    )
    .await?;
    Ok(())
}

/// What replaying a unit's work onto the tree it will land in came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rebase {
    /// The unit's work now sits on top of this tree — the ground it
    /// will actually land on, and therefore what its diff is judged
    /// against from here: the tree it opened on is no longer where its
    /// work is going. What it did is verified there, and then [`land`]
    /// moves the shared tree onto it.
    Onto(TreeId),
    /// git could not replay the unit's work where it has to go, and
    /// these are the paths it could not reconcile. The unit's own tree
    /// is left exactly as it was.
    ///
    /// Where two units of one run can be open at once, the static checks
    /// already proved their scopes disjoint, so this is a workflow that
    /// got past `check` rather than an ordinary outcome.
    Conflicts(Vec<PathBuf>),
}

/// Replays the unit's work onto `into` as it stands right now.
///
/// Only the unit's own commits move: the replay runs `--onto` from the
/// commit the unit was cut at, so whatever that commit carried — for a
/// unit opened over a tree somebody left uncommitted, that is the
/// leftovers themselves — stays where it was instead of arriving as the
/// unit's doing.
pub async fn rebase_onto(
    unit: &Unit,
    into: &Path,
    supervision: Supervision<'_>,
) -> Result<Rebase, WorktreeError> {
    let onto = head_commit(into, supervision).await?;
    if crate::git::success(
        &unit.worktree,
        &["rebase", "--onto", onto.as_str(), unit.base.as_str()],
        supervision,
    )
    .await?
    {
        // Asked of `into` and not of the rebased checkout: what the unit
        // now sits on is that tree, while its own `HEAD` already carries
        // the work a later diff has to find.
        return Ok(Rebase::Onto(super::head_tree(into, supervision).await?));
    }
    // Read while the rebase is still stopped on them: an abort is what
    // puts the tree back, and it takes the evidence with it.
    let conflicts = conflicted_paths(&unit.worktree, supervision).await;
    let _ = crate::git::success(&unit.worktree, &["rebase", "--abort"], supervision).await;
    Ok(Rebase::Conflicts(conflicts?))
}

/// Moves `into` onto the work the unit rebased there, and says where it
/// now stands.
///
/// Fast-forward only: this engine is the only writer of that tree
/// between the replay and here, so a merge with anything to reconcile
/// means that stopped being true, which is a broken engine and not a
/// verdict about the unit.
pub async fn land(
    unit: &Unit,
    into: &Path,
    supervision: Supervision<'_>,
) -> Result<CommitSha, WorktreeError> {
    let head = head_commit(&unit.worktree, supervision).await?;
    if !crate::git::success(into, &["merge", "--ff-only", head.as_str()], supervision).await? {
        return Err(WorktreeError::NotFastForward {
            unit: unit.who.to_string(),
            path: into.to_path_buf(),
        });
    }
    Ok(head)
}

/// The paths a stopped rebase could not reconcile, as git lists them
/// (`U` on either side of the merge).
async fn conflicted_paths(
    worktree: &Path,
    supervision: Supervision<'_>,
) -> Result<Vec<PathBuf>, WorktreeError> {
    let bytes = crate::git::output_bytes(
        worktree,
        &["diff", "--name-only", "--diff-filter=U", "-z"],
        supervision,
    )
    .await?;
    Ok(crate::scope::nul_separated_paths(&bytes))
}
