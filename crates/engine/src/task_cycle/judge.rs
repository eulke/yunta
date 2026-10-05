//! The one judgement a task's work gets: every criterion run again on
//! the tree it left, and the diff that tree carries audited against the
//! scope the task answers to.
//!
//! An attempt's close asks for it once the session has ended, and a task
//! session asks for it mid-session through `yunta_check_task`. Both come
//! here — the same criteria, the same cache, the same audit — so the
//! answer a session gets is the answer its close would give that tree.

use std::path::PathBuf;

use yunta_core::Task;

use super::criteria::{post_check, Memo, PostCheck};
use super::{CriterionRun, TaskCycleError};
use crate::process::Supervision;
use crate::scope::{audit_tree, ScopeCheckResult};
use crate::worktree::Unit;

/// What judging a task's work found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judgement {
    /// Every criterion that ran again (or was answered from the cache) on
    /// the tree as it stands.
    pub criteria: Vec<CriterionRun>,
    /// The guards that did not run, by command, because a criterion of
    /// the task's own is red: they run once every one of those passes.
    pub waiting: Vec<String>,
    /// Every path the work changed, and those outside its scope.
    pub scope: ScopeCheckResult,
}

impl Judgement {
    /// Whether this work closes its task: every criterion ran and passes,
    /// and nothing it changed lies outside its scope.
    pub fn closes(&self) -> bool {
        self.criteria.iter().all(|run| run.exit_code == 0)
            && self.waiting.is_empty()
            && self.scope.violations.is_empty()
    }
}

/// Where the work being judged lives, and what its audit leaves out.
pub(crate) struct Work<'a> {
    /// The checkout the work is in, and the tree it started from — what
    /// the diff is taken against, so an attempt answers for what an
    /// earlier one of its own left behind.
    pub unit: &'a Unit,
    /// What the adapter staged for its own mechanics, which no audit
    /// counts against the task.
    pub staged: &'a [PathBuf],
}

/// Judges `task`'s work: its criteria on `work`'s tree, then that tree's
/// diff against `ceiling` — the scope it declared plus what was granted,
/// never a request still waiting on a decision, and what no session of
/// the run may write.
pub(crate) async fn judge(
    task: &Task,
    ceiling: crate::scope::Ceiling<'_>,
    work: Work<'_>,
    memo: &Memo,
    supervision: Supervision<'_>,
) -> Result<Judgement, TaskCycleError> {
    let cwd = work.unit.worktree.as_path();
    let PostCheck {
        runs: criteria,
        waiting,
        tree,
    } = post_check(task, cwd, memo, supervision).await?;
    // The tree the criteria answered for is the tree the audit measures:
    // the checkout is read once.
    let scope = audit_tree(
        cwd,
        &work.unit.from,
        &tree,
        ceiling,
        work.staged,
        supervision,
    )
    .await?;
    Ok(Judgement {
        criteria,
        waiting,
        scope,
    })
}
