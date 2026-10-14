//! The sentences a handover gives a writer: why a criterion, a test or a spec
//! cannot be proven where the engine runs it, and what a spec written again
//! must keep.

use std::fmt::Display;

use yunta_core::events::ExecutionEnvironment;
use yunta_core::{TaskId, TasksFile};

use crate::task_cycle::CriterionRun;

/// What a spec written again for the departures breaks when it gives a task
/// departed from the tests it had.
pub(crate) fn spec_departed_unchanged() -> &'static str {
    "gives the task the tests a person accepted are wrong; write them again as the \
     departure and the person say"
}

/// What a spec written again breaks when it changes the tests of a task
/// nobody departed from.
pub(crate) fn spec_other_changed() -> &'static str {
    "nobody departed from this task's tests; hand its spec over as the run holds it"
}

/// What a spec written again breaks when it leaves out a task's spec the run
/// holds.
pub(crate) fn spec_left_out(task: &TaskId) -> String {
    format!("leaves out task `{task}`'s spec, which the run holds; hand it over as it is")
}

/// What a spec breaks when one of its files is a file the run's tree holds.
pub(crate) fn test_file_held(path: &str) -> String {
    format!(
        "`{path}` is a file the run's tree already holds; a test lives in a new file, beside \
         the project's own tests"
    )
}

/// What a finding's proposed criterion breaks when it already passes on the
/// run's tree.
pub(crate) fn proposed_passes(cmd: &str) -> String {
    format!(
        "`{cmd}` already exits 0 on the run's tree; a criterion that proves a fix fails until \
         the fix is in"
    )
}

/// What a criterion breaks when it already passes with no work done, with the
/// line it concluded with quoted after its verdict.
pub(crate) fn already_passes(cmd: &str, conclusion: &str) -> String {
    format!(
        "`{cmd}` already exits 0, before any work{conclusion}. A criterion that passes before \
         the work proves nothing: it must fail until the work is done — a test filter that \
         matches no test passes, and so does a test that checks nothing"
    )
}

/// What a guard breaks when it is already red with no work done, with the line
/// it concluded with quoted after its verdict.
pub(crate) fn guard_already_red(cmd: &str, exit_code: i32, conclusion: &str) -> String {
    format!("`{cmd}` already exits {exit_code}, before any work{conclusion}")
}

/// What a command that answered concluded with, quoted after its verdict:
/// the line a writer reads to see why it passed or failed.
pub(crate) fn quoted_conclusion(run: &CriterionRun) -> String {
    run.conclusion()
        .map(|line| format!(" — it said `{line}`"))
        .unwrap_or_default()
}

/// Why a criterion could not run, in the words a writer fixes it by:
/// what it exited with, what the shell said, and where it looked.
pub(crate) fn cannot_run(run: &CriterionRun, environment: Option<&ExecutionEnvironment>) -> String {
    let mut detail = format!(
        "`{}` exits {} where the engine runs criteria",
        run.cmd,
        run.exit_described()
    );
    if let Some(line) = run.conclusion() {
        detail.push_str(&format!(" — it said `{line}`"));
    }
    if let Some(environment) = environment {
        detail.push_str(&format!(" — under {environment}"));
    }
    detail
}

/// What a handover says when a test file could not be written into the
/// checkout its tests run in.
pub(crate) fn could_not_write(path: &str, source: impl Display) -> String {
    format!("could not write `{path}`: {source}")
}

/// Why a spec's task is not one the run's plan holds, naming the ones
/// it does.
pub(crate) fn unplanned(task: &TaskId, plan: Option<&TasksFile>) -> String {
    match plan {
        Some(plan) => format!(
            "the run's plan declares no task `{task}`; its tasks are {}",
            yunta_core::text::listed(plan.tasks.iter().map(|task| task.id.as_str()))
        ),
        None => "the run holds no plan for a spec to hold to".to_string(),
    }
}
