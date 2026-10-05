//! Every branch and ref name the engine composes.
//!
//! A run's own branch, under `refs/heads`, and the refs that keep the
//! work of units, under `refs/yunta`, where no list of branches shows
//! them. Units once kept their work on branches of their own; that
//! family's name is kept only to take such a branch's work in.

use yunta_core::RunId;

use super::UnitId;

/// Where a run's own branches live.
pub(crate) const RUN_BRANCHES: &str = "yunta/run";
/// Where units kept their work on branches, before refs outside
/// `refs/heads` kept it. Read only to take such a branch's work in.
pub(crate) const LEGACY_UNIT_BRANCHES: &str = "yunta/unit";
/// Where the refs that keep units' work live: outside `refs/heads`, so no
/// list of a person's branches shows them.
const UNIT_REFS: &str = "refs/yunta/unit";

/// The branch a run's own commits live on: the branch its worktree is
/// created on, what `{{run.branch}}` renders, what the run's cleanup
/// deletes, and what a diagnostic names when the checkout has to be
/// brought back. One name, composed here, so every one of those is the
/// same string.
pub fn run_branch(run_id: &RunId) -> String {
    format!("{RUN_BRANCHES}/{run_id}")
}

/// The ref that keeps the work of one attempt by one unit of work. The
/// unit works on no branch; what it commits is kept by this ref until it
/// lands — or, for a blocked task's work, until its run is collected.
///
/// It names the run because a checkout is the run's but a ref is the
/// whole repository's: two runs over one repository reach the same unit
/// id — the same plan run twice, a successor re-doing what its
/// predecessor left open — and the attempt number counts that run's log
/// alone, so both start at 1.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnitRef(String);

impl UnitRef {
    pub fn of(run_id: &RunId, who: &UnitId, attempt: u32) -> UnitRef {
        UnitRef(format!("{}{attempt}", Self::of_unit(run_id, who)))
    }

    /// The ref a name read back from git or a record is, when it is one.
    pub fn named(name: &str) -> Option<UnitRef> {
        name.starts_with(&format!("{UNIT_REFS}/"))
            .then(|| UnitRef(name.to_string()))
    }

    /// What every ref of `who`'s attempts in `run_id` starts with: the
    /// attempt number follows it.
    pub fn of_unit(run_id: &RunId, who: &UnitId) -> String {
        format!("{}{who}/", Self::of_run(run_id))
    }

    /// What every ref of a unit of `run_id` starts with.
    pub fn of_run(run_id: &RunId) -> String {
        format!("{UNIT_REFS}/{run_id}/")
    }

    pub fn name(&self) -> &str {
        &self.0
    }

    /// The attempt this ref keeps the work of.
    pub fn attempt(&self) -> Option<u32> {
        self.0.rsplit('/').next()?.parse().ok()
    }

    /// The ref that takes in what the legacy unit branch `branch`
    /// held — `yunta/unit/<run>/<unit>/<attempt>` — when it is one.
    pub(crate) fn of_legacy(branch: &str) -> Option<UnitRef> {
        let rest = branch.strip_prefix(&format!("{LEGACY_UNIT_BRANCHES}/"))?;
        Some(UnitRef(format!("{UNIT_REFS}/{rest}")))
    }
}

impl std::fmt::Display for UnitRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What every legacy unit branch of `run_id` starts with.
pub(crate) fn legacy_unit_branches_of(run_id: &RunId) -> String {
    format!("{LEGACY_UNIT_BRANCHES}/{run_id}/")
}

/// Whether `branch` is a legacy unit branch.
pub(crate) fn is_legacy_unit_branch(branch: &str) -> bool {
    branch.starts_with(&format!("{LEGACY_UNIT_BRANCHES}/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN: &str = "01JEXAMPLERUNID0000000000";

    #[test]
    fn a_unit_s_work_is_kept_outside_every_branch() {
        let kept = UnitRef::of(&RunId::from(RUN), &UnitId::Task("T001".into()), 1);
        assert!(!kept.name().starts_with("refs/heads/"), "{kept}");
        assert_eq!(kept.attempt(), Some(1));
        assert_eq!(UnitRef::named(kept.name()), Some(kept));
    }

    #[test]
    fn a_unit_ref_separates_two_runs_and_a_node_from_a_task() {
        let task = UnitId::Task("build".into());
        let first = UnitRef::of(&RunId::from("01JFIRSTRUN00000000000000"), &task, 1);
        let second = UnitRef::of(&RunId::from("01JSECONDRUN0000000000000"), &task, 1);
        let node = UnitRef::of(&RunId::from(RUN), &UnitId::Node("build".into()), 1);
        assert_ne!(first, second);
        assert_ne!(UnitRef::of(&RunId::from(RUN), &task, 1), node);
    }

    #[test]
    fn a_branch_of_the_old_layout_is_taken_in_by_the_ref_of_its_unit() {
        let branch = format!("yunta/unit/{RUN}/task/T001/2");
        let taken = UnitRef::of_legacy(&branch).unwrap();
        assert_eq!(
            taken,
            UnitRef::of(&RunId::from(RUN), &UnitId::Task("T001".into()), 2)
        );
        assert!(is_legacy_unit_branch(&branch));
        assert!(UnitRef::of_legacy(&run_branch(&RunId::from(RUN))).is_none());
    }
}
