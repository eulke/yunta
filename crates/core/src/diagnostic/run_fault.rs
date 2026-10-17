//! What the engine found when it ran a document's commands where the run
//! runs them, or read a spec against the plan and the tree the run holds.
//!
//! The words are written from these values wherever a reader meets them, so
//! a log keeps the exit code and the line a command said, not a sentence.

use serde::{Deserialize, Serialize};

use super::RuleCode;
use crate::events::{ExecutionEnvironment, Unanswered};
use crate::TaskId;

/// What the run found when it ran a document's commands, or read a spec
/// against the plan and the tree the run holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RunFault {
    /// A command that never answered: `said` is the line it concluded with,
    /// and `environment` the shell and `PATH` it ran under.
    CannotRun {
        cmd: String,
        exit_code: i32,
        said: Option<String>,
        environment: Option<ExecutionEnvironment>,
    },
    /// A program a criterion starts that is on no directory of the run's `PATH`.
    NotOnPath {
        cmd: String,
        program: String,
        environment: Option<ExecutionEnvironment>,
    },
    /// A criterion that passes before the work: `said` is the line it concluded with.
    AlreadyPasses { cmd: String, said: Option<String> },
    /// A guard that is red before the work: `said` is the line it concluded with.
    GuardAlreadyRed {
        cmd: String,
        exit_code: i32,
        said: Option<String>,
    },
    /// A finding's proposed criterion that already passes on the run's tree.
    ProposedPasses { cmd: String },
    /// `planned` is the plan's tasks; absent when the run holds no plan.
    UnplannedTask {
        task: TaskId,
        planned: Option<Vec<TaskId>>,
    },
    /// A test file a spec writes at a path the run's tree already holds.
    FileHeld { path: String },
    /// A spec written again for a departure gives the departed task the tests it had.
    DepartedUnchanged,
    /// A spec written again for a departure changes the tests of another task.
    OtherChanged,
    /// A spec written again for a departure leaves out a task's spec the run holds.
    LeftOut { task: TaskId },
}

impl RunFault {
    /// The stable name of the rule this fault breaks.
    pub fn code(&self) -> RuleCode {
        match self {
            RunFault::CannotRun { .. } | RunFault::NotOnPath { .. } => RuleCode::CriterionCannotRun,
            RunFault::AlreadyPasses { .. } => RuleCode::CriterionAlreadyPasses,
            RunFault::GuardAlreadyRed { .. } => RuleCode::GuardAlreadyRed,
            RunFault::ProposedPasses { .. } => RuleCode::ProposedCriterionAlreadyPasses,
            RunFault::UnplannedTask { .. } => RuleCode::UnknownSpecTask,
            RunFault::FileHeld { .. } => RuleCode::TestFileExists,
            RunFault::DepartedUnchanged => RuleCode::DepartedSpecUnchanged,
            RunFault::OtherChanged | RunFault::LeftOut { .. } => RuleCode::OtherSpecChanged,
        }
    }

    /// What the rule found, in the words a writer fixes it by.
    pub fn clause(&self) -> String {
        match self {
            RunFault::CannotRun {
                cmd,
                exit_code,
                said,
                environment,
            } => cannot_run(cmd, *exit_code, said.as_deref(), environment.as_ref()),
            RunFault::NotOnPath {
                cmd,
                program,
                environment,
            } => not_on_path(cmd, program, environment.as_ref()),
            RunFault::AlreadyPasses { cmd, said } => already_passes(cmd, said.as_deref()),
            RunFault::GuardAlreadyRed {
                cmd,
                exit_code,
                said,
            } => format!(
                "`{cmd}` already exits {exit_code}, before any work{}",
                said_line(said.as_deref())
            ),
            RunFault::ProposedPasses { cmd } => format!(
                "`{cmd}` already exits 0 on the run's tree; a criterion that proves a fix fails \
                 until the fix is in"
            ),
            RunFault::UnplannedTask { task, planned } => unplanned(task, planned.as_deref()),
            RunFault::FileHeld { path } => format!(
                "`{path}` is a file the run's tree already holds; a test lives in a new file, \
                 beside the project's own tests"
            ),
            RunFault::DepartedUnchanged => {
                "gives the task the tests a person accepted are wrong; write them again as the \
                 departure and the person say"
                    .to_string()
            }
            RunFault::OtherChanged => {
                "nobody departed from this task's tests; hand its spec over as the run holds it"
                    .to_string()
            }
            RunFault::LeftOut { task } => format!(
                "leaves out task `{task}`'s spec, which the run holds; hand it over as it is"
            ),
        }
    }
}

/// Why a command never answered: what it exited with, the line it concluded
/// with, and the shell and `PATH` it ran under.
fn cannot_run(
    cmd: &str,
    exit_code: i32,
    said: Option<&str>,
    environment: Option<&ExecutionEnvironment>,
) -> String {
    format!(
        "`{cmd}` exits {} where the engine runs criteria{}{}",
        Unanswered::exit_described(exit_code),
        said_line(said),
        under(environment)
    )
}

fn not_on_path(cmd: &str, program: &str, environment: Option<&ExecutionEnvironment>) -> String {
    format!(
        "`{cmd}` starts `{program}`, which is on no directory of the PATH the engine runs \
         criteria with{}",
        under(environment)
    )
}

fn already_passes(cmd: &str, said: Option<&str>) -> String {
    format!(
        "`{cmd}` already exits 0, before any work{}. A criterion that passes before the work \
         proves nothing: it must fail until the work is done — a test filter that matches no \
         test passes, and so does a test that checks nothing",
        said_line(said)
    )
}

fn unplanned(task: &TaskId, planned: Option<&[TaskId]>) -> String {
    match planned {
        Some(planned) => format!(
            "the run's plan declares no task `{task}`; its tasks are {}",
            crate::text::listed(planned.iter().map(TaskId::as_str))
        ),
        None => "the run holds no plan for a spec to hold to".to_string(),
    }
}

/// The line a command concluded with, quoted after its verdict; nothing when
/// it printed none.
fn said_line(said: Option<&str>) -> String {
    said.map(|line| format!(" — it said `{line}`"))
        .unwrap_or_default()
}

/// The shell and `PATH` a command ran under, after its verdict; nothing when
/// the run recorded no environment.
fn under(environment: Option<&ExecutionEnvironment>) -> String {
    environment
        .map(|environment| format!(" — under {environment}"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::RunFault;
    use crate::events::ExecutionEnvironment;
    use crate::TaskId;

    #[test]
    fn a_command_that_never_answered_reads_as_the_run_wrote_it() {
        let environment = ExecutionEnvironment {
            shell: "sh".to_string(),
            path: vec!["/usr/bin".to_string(), "/bin".to_string()],
        };
        assert_eq!(
            RunFault::CannotRun {
                cmd: "./check.sh".to_string(),
                exit_code: 127,
                said: Some("sh: ./check.sh: not found".to_string()),
                environment: Some(environment),
            }
            .clause(),
            "`./check.sh` exits 127 (command not found) where the engine runs criteria — it said \
             `sh: ./check.sh: not found` — under sh with PATH=/usr/bin:/bin"
        );
        assert_eq!(
            RunFault::NotOnPath {
                cmd: "cargo test".to_string(),
                program: "cargo".to_string(),
                environment: None,
            }
            .clause(),
            "`cargo test` starts `cargo`, which is on no directory of the PATH the engine runs \
             criteria with"
        );
    }

    #[test]
    fn a_criterion_that_already_passes_reads_as_the_run_wrote_it() {
        assert_eq!(
            RunFault::AlreadyPasses {
                cmd: "true".to_string(),
                said: None,
            }
            .clause(),
            "`true` already exits 0, before any work. A criterion that passes before the work \
             proves nothing: it must fail until the work is done — a test filter that matches no \
             test passes, and so does a test that checks nothing"
        );
        assert_eq!(
            RunFault::GuardAlreadyRed {
                cmd: "make lint".to_string(),
                exit_code: 2,
                said: None,
            }
            .clause(),
            "`make lint` already exits 2, before any work"
        );
    }

    #[test]
    fn a_plan_or_spec_gap_reads_as_the_run_wrote_it() {
        assert_eq!(
            RunFault::UnplannedTask {
                task: TaskId::from("t9"),
                planned: Some(vec![TaskId::from("t1"), TaskId::from("t2")]),
            }
            .clause(),
            "the run's plan declares no task `t9`; its tasks are `t1`, `t2`"
        );
        assert_eq!(
            RunFault::UnplannedTask {
                task: TaskId::from("t9"),
                planned: None,
            }
            .clause(),
            "the run holds no plan for a spec to hold to"
        );
        assert_eq!(
            RunFault::LeftOut {
                task: TaskId::from("t1"),
            }
            .clause(),
            "leaves out task `t1`'s spec, which the run holds; hand it over as it is"
        );
    }
}
