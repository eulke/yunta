//! A task held to its spec: the tests a spec gives it, laid over the tree
//! its work starts from before anything judges that work, denied to it,
//! and among the criteria that close it.
//!
//! A fresh cycle's guards answer for the tree before the tests are in it
//! — what passed before the run changed anything is what they promise —
//! and its other criteria for the tree with them, where each must fail:
//! the work is what makes them pass.

use std::path::PathBuf;

use yunta_core::events::TaskLedger;
use yunta_core::{Task, TestFile};

use super::criteria::pre_check_unless_cut;
use super::{CriterionRun, Memo, SessionSetup, TaskCycleError};
use crate::process::Supervision;
use crate::worktree::Unit;

/// Where a cycle starts: its task, the unit it opened in, and whether it
/// starts from that unit's own tree or picks up work already made.
pub(super) struct Start<'a> {
    pub(super) task: &'a Task,
    pub(super) unit: &'a Unit,
    pub(super) setup: &'a SessionSetup,
    /// Neither carried work nor a resumed session: the pre-check is the
    /// cycle's own.
    pub(super) fresh: bool,
    /// The unit is about to take a blocked task's committed work, which
    /// brings the files with it.
    pub(super) carried: bool,
}

/// A cycle's start once its task's tests are in it: the unit judged from
/// its start with them laid over it, and what a fresh cycle's guards
/// answered before they were.
pub(super) struct Laid {
    pub(super) unit: Unit,
    pub(super) guarded: Option<Vec<CriterionRun>>,
}

/// Lays `start`'s tests over its unit: a fresh cycle's guards answer
/// first, for the tree before they are in it. `None` when a cancellation
/// cut the guards before they answered.
pub(super) async fn laid(
    start: Start<'_>,
    memo: &Memo,
    history: &TaskLedger,
    supervision: Supervision<'_>,
) -> Result<Option<Laid>, TaskCycleError> {
    let Start {
        task,
        unit,
        setup,
        fresh,
        carried,
    } = start;
    let files = files(setup, task);
    let guarded = match fresh && !files.is_empty() {
        true => {
            let guards = only(task, true);
            match pre_check_unless_cut(&guards, &unit.worktree, memo, history, supervision).await? {
                Some(runs) => Some(runs),
                None => return Ok(None),
            }
        }
        false => None,
    };
    let unit = overlaid(task, unit, files, setup, supervision).await?;
    // Carried work brings the files with it, so they go in after it.
    if !carried {
        write(task, &unit, setup).await?;
    }
    Ok(Some(Laid { unit, guarded }))
}

/// The files the run's spec gives `task`: none when the run holds no
/// spec, or its spec gives the task none.
pub(super) fn files<'a>(setup: &'a SessionSetup, task: &Task) -> &'a [TestFile] {
    setup
        .spec
        .as_deref()
        .and_then(|spec| spec.of(&task.id))
        .map_or(&[], |spec| spec.files.as_slice())
}

/// Where `files` sit in the repository, as the paths a diff names.
pub(super) fn paths(files: &[TestFile]) -> impl Iterator<Item = PathBuf> + '_ {
    files.iter().map(|file| PathBuf::from(file.in_repo()))
}

/// `unit`, judged from the tree it started from with `files` in it.
async fn overlaid(
    task: &Task,
    unit: &Unit,
    files: &[TestFile],
    setup: &SessionSetup,
    supervision: Supervision<'_>,
) -> Result<Unit, TaskCycleError> {
    if files.is_empty() {
        return Ok(unit.clone());
    }
    let index = crate::run_dir::index_for(&setup.run_dir, &unit.who).with_extension("spec");
    let from = crate::worktree::tree_with(&unit.worktree, &index, &unit.from, files, supervision)
        .await
        .map_err(|source| failed(task, source))?;
    Ok(Unit {
        from,
        ..unit.clone()
    })
}

/// Writes the files the run's spec gives `task` into `unit`'s checkout,
/// over whatever is there, once the files only an earlier spec gave it
/// are out of it.
pub(super) async fn write(
    task: &Task,
    unit: &Unit,
    setup: &SessionSetup,
) -> Result<(), TaskCycleError> {
    let gone = setup
        .superseded
        .get(&task.id)
        .map_or(&[][..], Vec::as_slice);
    crate::worktree::remove_files(&unit.worktree, gone)
        .await
        .map_err(|source| failed(task, source))?;
    crate::worktree::write_files(&unit.worktree, files(setup, task))
        .await
        .map_err(|source| failed(task, source))
}

/// `task` with only its guards, or only its other criteria.
pub(super) fn only(task: &Task, guards: bool) -> Task {
    Task {
        criteria: task
            .criteria
            .iter()
            .filter(|criterion| criterion.is_guard() == guards)
            .cloned()
            .collect(),
        ..task.clone()
    }
}

fn failed(task: &Task, source: crate::worktree::WorktreeError) -> TaskCycleError {
    TaskCycleError::Spec {
        task: task.id.clone(),
        source: Box::new(source),
    }
}
