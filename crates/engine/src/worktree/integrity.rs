//! Whether a run's worktree is still the tree the run's own history
//! describes.
//!
//! **Content is never the question.** An artifact is immutable, so
//! verifying one asks "are these still the bytes the run accepted", and a
//! difference is corruption with no other reading. A worktree is the
//! opposite: it *is* the work, and it changes by design. Between a pause
//! and a resume a person legitimately opens it, fixes something by hand,
//! runs the tests, commits — and at a gate that is precisely what the run
//! is waiting for. A rule of the form "the worktree must hash as it did
//! at the pause" would report the normal use of the system as corruption.
//! The engine already answers a moved tree the right way elsewhere:
//! `task_cycle::criteria` keys its memo on a hash of the whole tree, so a
//! tree that changed re-runs its criteria instead of reusing a result
//! about a tree that is gone. Re-verify is the answer to something
//! mutable; reject is the answer to something immutable.
//!
//! What is left is what cannot legitimately change, and both are cheap.
//!
//! **Identity** — the worktree is at the path the manifest froze, and git
//! knows it as a working tree. Everything downstream (the scope diff, the
//! criteria's tree hash, each task's own checkout) runs git *in* that
//! directory, so a directory that is not one turns every later step into
//! the same failure with a worse message. The question is only whether
//! git works there: which linked worktree it is, and where it sits inside
//! the repository, is the manifest's frozen answer and not something to
//! re-derive.
//!
//! **Ancestry** — the run's `base_commit` is still reachable from the
//! worktree's HEAD. Every task marked done, every `scope_checked`, every
//! green criterion on the run's log was established against a tree
//! descending from that commit. Once HEAD stops descending from it —
//! a `reset --hard` behind the run's own commits, a rebase, a checkout of
//! an unrelated branch — the state derived by replay describes a tree
//! that no longer exists, and no amount of re-verifying reconciles them.
//! That is the one unambiguous inconsistency a worktree can carry, and
//! one `git merge-base --is-ancestor` answers it.

use std::path::{Path, PathBuf};

use yunta_core::{CommitSha, Isolation, RunId};

use super::{head_commit, run_branch, WorktreeError};
use crate::process::Supervision;

/// The worktree a run works in, exactly as the run's manifest froze it.
#[derive(Debug, Clone, Copy)]
pub struct RunWorktree<'a> {
    /// The run this worktree belongs to — it names the run's branch and
    /// every diagnostic about the tree.
    pub run_id: &'a RunId,
    /// Where the tree is: the run's own linked worktree under
    /// [`Isolation::Worktree`], the checkout the run operates on directly
    /// under [`Isolation::None`].
    pub path: &'a Path,
    /// The commit the run branched from.
    pub base_commit: &'a CommitSha,
    pub isolation: Isolation,
}

/// A run's worktree read back: where its HEAD is now, and whether the
/// run's base commit is still behind it.
///
/// Constructing one is the identity check — [`WorktreeIntegrity::of`]
/// fails when there is no working tree to ask — and reading
/// [`diagnostic`](WorktreeIntegrity::diagnostic) is the ancestry one.
#[derive(Debug)]
pub struct WorktreeIntegrity {
    run_id: RunId,
    path: PathBuf,
    base_commit: CommitSha,
    isolation: Isolation,
    /// Where the worktree's HEAD is now. A run that did any work has
    /// moved it, which is the expected case and not a finding.
    head: CommitSha,
    /// Whether `base_commit` is still in the history of `head`.
    descends_from_base: bool,
}

impl WorktreeIntegrity {
    /// Reads `run`'s worktree back.
    ///
    /// Errors when there is no working tree at the frozen path to ask —
    /// the run's own evidence is untouched in that case, so the remedy is
    /// to put the checkout back, which the error says how to do.
    pub async fn of(
        run: RunWorktree<'_>,
        supervision: Supervision<'_>,
    ) -> Result<Self, WorktreeError> {
        require_working_tree(run, supervision).await?;
        let head = head_commit(run.path, supervision).await?;
        // A non-zero exit is the answer "no", not a failure: git says the
        // same when the base commit is not an ancestor and when this
        // repository does not have that commit at all, and both mean the
        // run's history is not behind this HEAD.
        let descends_from_base = crate::git::success(
            run.path,
            &[
                "merge-base",
                "--is-ancestor",
                run.base_commit.as_str(),
                head.as_str(),
            ],
            supervision,
        )
        .await
        .map_err(WorktreeError::Git)?;
        Ok(WorktreeIntegrity {
            run_id: run.run_id.clone(),
            path: run.path.to_path_buf(),
            base_commit: run.base_commit.clone(),
            isolation: run.isolation,
            head,
            descends_from_base,
        })
    }

    /// Where the worktree's HEAD is now.
    pub fn head(&self) -> &CommitSha {
        &self.head
    }

    /// Why the state derived from the run's log no longer describes this
    /// worktree, or `None` when the run's base commit is still behind its
    /// HEAD.
    ///
    /// The sentence names the three things a reader needs to tell what
    /// happened apart from what to do about it: the tree, the commit the
    /// run branched from, and the commit the tree is on now.
    pub fn diagnostic(&self) -> Option<String> {
        if self.descends_from_base {
            return None;
        }
        Some(format!(
            "run `{run}` works in `{path}`, whose HEAD `{head}` no longer has the run's base \
             commit `{base}` behind it: every task, scope check and criterion this run's log \
             records was established against a tree that this one is not a continuation of. \
             {remedy}",
            run = self.run_id,
            path = self.path.display(),
            head = self.head,
            base = self.base_commit,
            remedy = self.remedy(),
        ))
    }

    /// What to do about a worktree the run's history has lost: put the
    /// tree back where the run left it, or accept that this run's history
    /// is over and start one from the tree as it is.
    fn remedy(&self) -> String {
        match self.isolation {
            Isolation::Worktree => format!(
                "Put it back on the run's own branch (`git -C {path} checkout {branch}`), or on \
                 any commit that still descends from `{base}` — `git -C {path} reflog` shows \
                 where the run left it. If those commits are gone for good, start a new run \
                 against the tree as it is now.",
                path = self.path.display(),
                branch = run_branch(&self.run_id),
                base = self.base_commit,
            ),
            Isolation::None => format!(
                "This run has `isolation: none`, so it works directly on this checkout: put it \
                 back on a commit that still descends from `{base}` — `git -C {path} reflog` \
                 shows where the run left it, and a rebase or a pull under a paused run is the \
                 usual way it moves off. If that history is gone for good, start a new run \
                 against the tree as it is now.",
                path = self.path.display(),
                base = self.base_commit,
            ),
        }
    }
}

/// The identity half: there is a directory at the frozen path, and git
/// knows it as a working tree.
async fn require_working_tree(
    run: RunWorktree<'_>,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    let present = tokio::fs::try_exists(run.path)
        .await
        .map_err(|source| WorktreeError::Io {
            action: "look for the run's worktree".to_string(),
            path: run.path.to_path_buf(),
            source,
        })?;
    let detail = if present {
        match crate::git::output(
            run.path,
            &["rev-parse", "--is-inside-work-tree"],
            supervision,
        )
        .await
        {
            Ok(answer) if answer.trim() == "true" => return Ok(()),
            Ok(answer) => format!("git answers `{}` there, not `true`", answer.trim()),
            Err(e) => e.detail(),
        }
    } else {
        "there is nothing at that path".to_string()
    };
    Err(match run.isolation {
        Isolation::Worktree => WorktreeError::RunWorktreeLost {
            path: run.path.to_path_buf(),
            branch: run_branch(run.run_id),
            detail,
        },
        Isolation::None => WorktreeError::NotACheckout {
            path: run.path.to_path_buf(),
            detail,
        },
    })
}
