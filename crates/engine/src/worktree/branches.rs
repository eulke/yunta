//! Every branch name the engine composes.
//!
//! Two families — a run's own branch, and the branch of one attempt by
//! one unit of work — that share a repository's ref namespace and therefore have
//! to be designed together. Git refuses a ref that is a directory of
//! another (`refs/heads/a` and `refs/heads/a/b` never coexist, in either
//! order), so the two diverge at the segment right after `yunta/`, both
//! of them literals this module chooses. Diverging there is what makes
//! them coexist for every run id, rather than for the ones that happen
//! not to collide.

use yunta_core::RunId;

use super::UnitId;

/// Where a run's own branches live.
const RUN_BRANCHES: &str = "yunta/run";
/// Where the branches of units' attempts live.
const UNIT_BRANCHES: &str = "yunta/unit";

/// The branch a run's own commits live on: the branch its worktree is
/// created on, what `{{run.branch}}` renders, what the run's cleanup
/// deletes, and what a diagnostic names when the checkout has to be
/// brought back. One name, composed here, so every one of those is the
/// same string.
pub fn run_branch(run_id: &RunId) -> String {
    format!("{RUN_BRANCHES}/{run_id}")
}

/// The branch one attempt by one unit of work works on, inside the tree
/// that unit was opened in.
///
/// It names the run because a worktree is the run's but a ref is the
/// whole repository's: two runs over one checkout reach the same unit id
/// — the same plan run twice, a successor re-doing what its predecessor
/// left open — and the attempt number counts that run's log alone, so
/// both start at 1. A name built from the unit alone asks git for one
/// branch twice, and the second run cannot have it.
pub fn unit_branch(run_id: &RunId, who: &UnitId, attempt: u32) -> String {
    format!("{UNIT_BRANCHES}/{run_id}/{who}/{attempt}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neither_family_is_reachable_by_walking_into_the_other() {
        let run = RunId::from("01JEXAMPLERUNID0000000000");
        let unit = unit_branch(&run, &UnitId::Task("T001".into()), 1);
        assert!(
            !unit.starts_with(&format!("{}/", run_branch(&run))),
            "a unit branch under the run's own branch is a ref git cannot create: {unit}"
        );
        assert!(
            !run_branch(&run).starts_with(&format!("{unit}/")),
            "and neither direction of the same conflict"
        );
    }

    #[test]
    fn a_unit_branch_separates_two_runs_at_the_same_unit_and_attempt() {
        let unit = UnitId::Task("T001".into());
        assert_ne!(
            unit_branch(&RunId::from("01JFIRSTRUN00000000000000"), &unit, 1),
            unit_branch(&RunId::from("01JSECONDRUN0000000000000"), &unit, 1),
        );
    }

    #[test]
    fn a_node_and_a_task_of_one_name_get_their_own_branches() {
        let run = RunId::from("01JEXAMPLERUNID0000000000");
        assert_ne!(
            unit_branch(&run, &UnitId::Node("build".into()), 1),
            unit_branch(&run, &UnitId::Task("build".into()), 1),
        );
    }
}
