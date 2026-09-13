//! Every branch name the engine composes.
//!
//! Two families — a run's own branch, and the branch of one attempt at
//! one task — that share a repository's ref namespace and therefore have
//! to be designed together. Git refuses a ref that is a directory of
//! another (`refs/heads/a` and `refs/heads/a/b` never coexist, in either
//! order), so the two diverge at the segment right after `yunta/`, both
//! of them literals this module chooses. Diverging there is what makes
//! them coexist for every run id, rather than for the ones that happen
//! not to collide.

use yunta_core::{RunId, TaskId};

/// Where a run's own branches live.
const RUN_BRANCHES: &str = "yunta/run";
/// Where the branches of task attempts live.
const TASK_BRANCHES: &str = "yunta/task";

/// The branch a run's own commits live on: the branch its worktree is
/// created on, what `{{run.branch}}` renders, what the run's cleanup
/// deletes, and what a diagnostic names when the checkout has to be
/// brought back. One name, composed here, so every one of those is the
/// same string.
pub fn run_branch(run_id: &RunId) -> String {
    format!("{RUN_BRANCHES}/{run_id}")
}

/// The branch one attempt at one task works on, inside the worktree the
/// loop gives that attempt.
///
/// It names the run because a worktree is the run's but a ref is the
/// whole repository's: two runs over one checkout reach the same task id
/// — the same plan run twice, a successor re-doing what its predecessor
/// left open — and the attempt number counts that run's log alone, so
/// both start at 1. A name built from the task alone asks git for one
/// branch twice, and the second run cannot have it.
pub fn task_branch(run_id: &RunId, task_id: &TaskId, attempt: u32) -> String {
    format!("{TASK_BRANCHES}/{run_id}/{task_id}/{attempt}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neither_family_is_reachable_by_walking_into_the_other() {
        let run = RunId::from("01JEXAMPLERUNID0000000000");
        let task = task_branch(&run, &"T001".into(), 1);
        assert!(
            !task.starts_with(&format!("{}/", run_branch(&run))),
            "a task branch under the run's own branch is a ref git cannot create: {task}"
        );
        assert!(
            !run_branch(&run).starts_with(&format!("{task}/")),
            "and neither direction of the same conflict"
        );
    }

    #[test]
    fn a_task_branch_separates_two_runs_at_the_same_task_and_attempt() {
        let task = TaskId::from("T001");
        assert_ne!(
            task_branch(&RunId::from("01JFIRSTRUN00000000000000"), &task, 1),
            task_branch(&RunId::from("01JSECONDRUN0000000000000"), &task, 1),
        );
    }
}
