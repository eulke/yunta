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

use yunta_core::protected::Pathspecs;
use yunta_core::{CommitSha, NodeId, RunId, TaskId, TreeId};

use super::{head_commit, WorktreeError};
use crate::git::{self, Effect, Exits, GitError, IS_ANCESTOR};
use crate::process::Supervision;

/// What `diff --cached --quiet` exits with when nothing is staged: 0. A
/// staged change exits 1.
const NOTHING_STAGED: Exits = Exits { yes: 0, no: 1 };

/// Whether `node`'s work is done by units of its own — a loop's tasks,
/// or an audited node — and lands on the run's tree once it passes.
pub fn lands_units(node: &yunta_core::Node) -> bool {
    matches!(node.kind, yunta_core::NodeKind::Loop { .. }) || crate::scope::audits(node)
}

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
    /// The ref that keeps what it commits: it works on no branch.
    pub keeps: super::UnitRef,
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
/// plus whatever is lying in its working tree uncommitted, less what git
/// ignores.
///
/// A unit branches from what it would have found, not from the last
/// thing anybody committed: a node's close commits what it left in the
/// run's tree, but a person may have written there since, and a run
/// working in a person's own checkout commits nothing. Nothing points at
/// this commit but the unit's own branch: `repo`'s branch does not move,
/// and what is uncommitted there stays uncommitted, to be landed over
/// later.
pub async fn snapshot_commit(
    repo: &Path,
    index: &Path,
    message: &str,
    supervision: Supervision<'_>,
) -> Result<CommitSha, WorktreeError> {
    let tree = super::capture::capture_for_commit(repo, index, supervision).await?;
    let head = head_commit(repo, supervision).await?;
    let printed = git::change(
        repo,
        Effect::Objects,
        &[
            "commit-tree",
            tree.as_str(),
            "-p",
            head.as_str(),
            "-m",
            message,
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

/// Puts the work committed at `left` back as uncommitted changes on the
/// commit it was made on — where an attempt's own edits sit — and
/// answers that commit.
pub(super) async fn back_to_base(
    worktree: &Path,
    left: &CommitSha,
    supervision: Supervision<'_>,
) -> Result<CommitSha, WorktreeError> {
    let parent = format!("{}^", left.as_str());
    let base: CommitSha = git::read(worktree, &["rev-parse", parent.as_str()], supervision)
        .await?
        .trim()
        .parse()
        .map_err(|source| WorktreeError::NotACommit {
            args: format!("rev-parse {parent}"),
            cwd: worktree.to_path_buf(),
            source,
        })?;
    git::change(
        worktree,
        Effect::Checkout,
        &["reset", "-q", base.as_str()],
        supervision,
    )
    .await?;
    Ok(base)
}

/// What putting a blocked task's work back into a fresh unit came to.
#[derive(Debug)]
pub enum Carried {
    /// The work is in the checkout, as uncommitted changes.
    Applied,
    /// It no longer applies on the tree the unit began from: `stopped` is
    /// the pick git stopped on, and `paths` are the paths it could not
    /// reconcile. The checkout is back as it began.
    NoLongerApplies {
        stopped: GitError,
        paths: Vec<PathBuf>,
    },
}

/// Puts the work `left` holds into `unit`'s checkout as uncommitted
/// changes on top of the tree the unit began from — exactly where an
/// attempt's own edits sit, so the unit's audit answers for all of it.
/// When it no longer applies there, the checkout is put back as it
/// began, without the ignored paths `covered` reaches, and the paths it
/// stopped on are named. A pick git refuses without leaving a path
/// unmerged is an error, and the checkout is left as it was.
pub async fn carry_work(
    unit: &Unit,
    left: &CommitSha,
    covered: &Pathspecs,
    supervision: Supervision<'_>,
) -> Result<Carried, WorktreeError> {
    let tree = unit.worktree.as_path();
    let picked = ["cherry-pick", "--no-commit", left.as_str()];
    match git::change(tree, Effect::Checkout, &picked, supervision).await {
        Ok(_) => {
            git::change(tree, Effect::Checkout, &["reset", "-q"], supervision).await?;
            Ok(Carried::Applied)
        }
        Err(refused) if refused.cancelled() => Err(refused.into()),
        Err(stopped) => {
            // Read while the pick is still stopped on them: the undo below is
            // what puts the tree back, and it takes the evidence with it.
            let paths = conflicted_paths(tree, supervision).await?;
            if paths.is_empty() {
                // git refused the pick before it applied anything: nothing to
                // undo, and the checkout is as it was.
                return Err(stopped.into());
            }
            // `--no-commit` records no pick in progress, so there is no
            // `cherry-pick --abort` to run. The reset puts the index and the
            // tree back on the base, which clears the conflict, and the clean
            // removes what the pick added. The clean of the protected paths
            // takes the ignored ones the plain clean keeps. Each must succeed
            // for the checkout to be back as it began.
            for undo in [
                ["reset", "-q", "--hard", unit.base.as_str()].as_slice(),
                ["clean", "-q", "-fd"].as_slice(),
            ] {
                git::change(tree, Effect::Checkout, undo, supervision).await?;
            }
            super::capture::clean_covered(tree, covered, supervision).await?;
            Ok(Carried::NoLongerApplies { stopped, paths })
        }
    }
}

/// Commits everything the unit did, under `message`, and points the
/// unit's ref at it: the unit works on no branch, and the ref is what
/// keeps the commit when its checkout goes to somebody else.
///
/// A unit that changed nothing produces no commit and is not an error:
/// work whose criteria are met by side effects that leave no diff is
/// still work done. What git ignores is never committed.
pub async fn commit_work(
    unit: &Unit,
    message: &str,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    git::change(
        &unit.worktree,
        Effect::Checkout,
        &["add", "-A"],
        supervision,
    )
    .await?;
    if git::ask(
        &unit.worktree,
        &["diff", "--cached", "--quiet"],
        NOTHING_STAGED,
        supervision,
    )
    .await?
    {
        return Ok(());
    }
    git::change(
        &unit.worktree,
        Effect::Checkout,
        &["commit", "-q", "-m", message],
        supervision,
    )
    .await?;
    let keep = ["update-ref", unit.keeps.name(), "HEAD"];
    git::change(&unit.worktree, Effect::Refs, &keep, supervision).await?;
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
    let rebase = ["rebase", "--onto", onto.as_str(), unit.base.as_str()];
    match git::change(&unit.worktree, Effect::Checkout, &rebase, supervision).await {
        Ok(_) => {
            // Asked of `into` and not of the rebased checkout: what the unit
            // now sits on is that tree, while its own `HEAD` already carries
            // the work a later diff has to find.
            Ok(Rebase::Onto(super::head_tree(into, supervision).await?))
        }
        Err(refused) if refused.cancelled() => Err(refused.into()),
        Err(refused) => {
            // Read while the rebase is still stopped on them: an abort is
            // what puts the tree back, and it takes the evidence with it.
            let paths = conflicted_paths(&unit.worktree, supervision).await?;
            if paths.is_empty() {
                // git refused before it started: a dirty tree, or an index
                // lock held by someone else. Nothing conflicts.
                return Err(refused.into());
            }
            git::change(
                &unit.worktree,
                Effect::Checkout,
                &["rebase", "--abort"],
                supervision,
            )
            .await?;
            Ok(Rebase::Conflicts(paths))
        }
    }
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
    if git::verify(into, head.as_str(), supervision)
        .await?
        .is_none()
    {
        return Err(WorktreeError::UnknownCommit { commit: head });
    }
    let merge = ["merge", "--ff-only", head.as_str()];
    match git::change(into, Effect::Checkout, &merge, supervision).await {
        Ok(_) => Ok(head),
        Err(refused) if refused.cancelled() => Err(refused.into()),
        Err(refused) => {
            // A refused fast-forward has two causes that look alike. When
            // `into`'s head is not behind the unit's head, `into` moved off
            // the commit the unit grew from. When it is, the merge failed
            // for another reason, such as a dirty checkout.
            let into_head = head_commit(into, supervision).await?;
            let ancestry = [
                "merge-base",
                "--is-ancestor",
                into_head.as_str(),
                head.as_str(),
            ];
            match git::ask(into, &ancestry, IS_ANCESTOR, supervision).await {
                Ok(false) => Err(WorktreeError::NotFastForward {
                    unit: unit.who.to_string(),
                    path: into.to_path_buf(),
                }),
                Err(asked) if asked.cancelled() => Err(asked.into()),
                Ok(true) => Err(refused.into()),
                Err(asked) => {
                    tracing::debug!(%asked, "the ancestry of a refused landing could not be asked");
                    Err(refused.into())
                }
            }
        }
    }
}

/// The paths a stopped rebase could not reconcile, as git lists them
/// (`U` on either side of the merge).
async fn conflicted_paths(
    worktree: &Path,
    supervision: Supervision<'_>,
) -> Result<Vec<PathBuf>, WorktreeError> {
    let bytes = git::read_bytes(
        worktree,
        &["diff", "--name-only", "--diff-filter=U", "-z"],
        supervision,
    )
    .await?;
    Ok(crate::scope::nul_separated_paths(&bytes))
}
