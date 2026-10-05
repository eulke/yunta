//! Whether a plan's criteria can run, asked without running them.
//!
//! A run that writes a spec proves its plan's tests there: they fail when
//! the spec hands them over, with every file they read in place. Before
//! that the tests a criterion runs are not written, so running it says
//! nothing — a filter that matches no test passes. What the plan can be
//! held to when it is handed over is that each program its criteria start
//! is one the engine finds, looked up the way it runs criteria.

use std::collections::BTreeSet;
use std::path::Path;

use yunta_core::diagnostic::{Diagnostic, Named, Problem, RuleCode, Subject};
use yunta_core::events::ExecutionEnvironment;
use yunta_core::{Task, TasksFile};

use super::session::RunToolError;
use crate::process::{spawn_governed, GovernedCommand, Outcome, Supervision};

/// Every criterion of the tasks in `tasks` that `asked` keeps which starts
/// a program the run's commands cannot find from `cwd`: one shell asks
/// for them all, and nothing a criterion says is run.
pub(super) async fn unfindable(
    tasks: &TasksFile,
    asked: impl Fn(&Task) -> bool,
    cwd: &Path,
    supervision: Supervision<'_>,
    environment: Option<&ExecutionEnvironment>,
) -> Result<Vec<Diagnostic>, RunToolError> {
    let programs: BTreeSet<String> = tasks
        .tasks
        .iter()
        .filter(|task| asked(task))
        .flat_map(|task| task.criteria.iter())
        .flat_map(|criterion| crate::check::leading_programs(&criterion.cmd))
        .collect();
    if programs.is_empty() {
        return Ok(Vec::new());
    }
    let missing = missing(&programs, cwd, supervision).await?;
    let mut found = Vec::new();
    for (index, task) in tasks
        .tasks
        .iter()
        .enumerate()
        .filter(|(_, task)| asked(task))
    {
        for (at, criterion) in task.criteria.iter().enumerate() {
            let lacking = crate::check::leading_programs(&criterion.cmd)
                .into_iter()
                .find(|program| missing.contains(program));
            if let Some(program) = lacking {
                let mut detail = format!(
                    "`{}` starts `{program}`, which is on no directory of the PATH the engine runs \
                     criteria with",
                    criterion.cmd
                );
                if let Some(environment) = environment {
                    detail.push_str(&format!(" — under {environment}"));
                }
                found.push(Diagnostic::new(
                    Subject::Criterion {
                        task: Named::new(task.id.clone(), index),
                        index: at,
                    },
                    Problem::rule(RuleCode::CriterionCannotRun, detail),
                ));
            }
        }
    }
    Ok(found)
}

/// The names in `programs` the run's shell finds nowhere.
async fn missing(
    programs: &BTreeSet<String>,
    cwd: &Path,
    supervision: Supervision<'_>,
) -> Result<BTreeSet<String>, RunToolError> {
    let names = programs.iter().cloned().collect::<Vec<_>>().join(" ");
    let script = format!(
        "for program in {names}; do command -v \"$program\" >/dev/null || echo \"$program\"; done"
    );
    let failed = |detail: String| RunToolError::Handover { detail };
    match spawn_governed(GovernedCommand::shell(cwd, &script), supervision).await {
        Ok(Outcome::Exited { stdout, .. }) => Ok(String::from_utf8_lossy(&stdout)
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect()),
        Ok(_) => Err(failed(
            "the lookup of the plan's programs was stopped".to_string(),
        )),
        Err(source) => Err(failed(source.to_string())),
    }
}
