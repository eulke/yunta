//! The tools that speak about the run's tasks document: where its tasks
//! stand, and what one task session's task is and how its work stands.
//!
//! None of them writes the document or a task's status. A task session
//! reads its task here because this is the only place it can: the
//! document lives with the run, not in the checkout, and a copy of it in
//! the session's prompt would be a second source that could disagree
//! with the one the engine judges by. What `yunta_task` answers is the
//! task the cycle holds and the checks the log holds; what
//! `yunta_check_task` answers is the judgement the attempt's close makes.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;
use yunta_core::events::{
    CriterionResult, CriterionType, EventPayload, ExecutionEnvironment, NodeEvent, Phase,
    StoredEvent, TaskEvent, TaskStatus,
};
use yunta_core::{ScopeGlob, TaskId};

use super::catalog::RunTool;
use super::host::TaskAccess;
use super::session::{RunToolError, SessionTools};
use crate::task_cycle::{could_not_run, judge, CriterionRun, Work};

impl SessionTools {
    pub(super) async fn task_status(&self) -> Result<String, RunToolError> {
        let state = crate::replay::derive(&self.events().await?);
        let mut tasks: Vec<(String, String)> = state
            .tasks
            .iter()
            .map(|(id, record)| {
                let status = record.status;
                (
                    id.to_string(),
                    serde_json::to_value(status)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_string))
                        .unwrap_or_else(|| format!("{status:?}")),
                )
            })
            .collect();
        tasks.sort();
        let map: serde_json::Map<String, Value> = tasks
            .into_iter()
            .map(|(id, status)| (id, Value::String(status)))
            .collect();
        serde_json::to_string_pretty(&Value::Object(map))
            .map_err(|source| RunToolError::Render { source })
    }

    /// This session's task as the cycle judges it — its scope, criteria
    /// and notes — with every check the log holds of its current cycle.
    pub(super) async fn task(&self) -> Result<String, RunToolError> {
        let access = self.task_access(RunTool::Task)?;
        let events = self.events().await?;
        let task = &access.task;
        let sheet = TaskSheet {
            id: &task.id,
            title: &task.title,
            notes: task
                .notes
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty()),
            description: task
                .description
                .as_deref()
                .map(str::trim)
                .filter(|d| !d.is_empty()),
            depends_on: &task.depends_on,
            scope: &access.scope,
            criteria: task
                .criteria
                .iter()
                .map(|criterion| Declared {
                    cmd: &criterion.cmd,
                    guard: criterion.r#type == Some(CriterionType::Guard),
                    proves: criterion.proves.as_deref(),
                })
                .collect(),
            cycles: cycles_of(&events, &task.id),
            runs_under: None,
        };
        let any_unrunnable = sheet
            .cycles
            .iter()
            .flat_map(|cycle| &cycle.checks)
            .flat_map(|check| &check.criteria)
            .any(|answered| answered.cannot_run.is_some());
        let sheet = TaskSheet {
            runs_under: self.host.environment.as_ref().filter(|_| any_unrunnable),
            ..sheet
        };
        render(&sheet)
    }

    /// The judgement this session's attempt would get if it ended now.
    pub(super) async fn check_task(&self) -> Result<String, RunToolError> {
        let access = self.task_access(RunTool::CheckTask)?;
        let judgement = judge(
            &access.task,
            crate::scope::Ceiling {
                scope: &access.scope,
                deny: &access.denied,
            },
            Work {
                unit: &access.unit,
                index: &access.index,
                staged: access.staged.get().map_or(&[], Vec::as_slice),
            },
            &self.host.memo,
            self.host.supervision(&self.stop),
        )
        .await
        .map_err(|source| RunToolError::Check { source })?;
        let any_unrunnable = judgement
            .criteria
            .iter()
            .any(|run| run.could_not_run().is_some());
        render(&Verdict {
            closes: judgement.closes(),
            criteria: judgement
                .criteria
                .iter()
                .map(|run| Answered::of_run(run, &self.host.redactor))
                .collect(),
            outside_scope: judgement.scope.violations,
            runs_under: self.host.environment.as_ref().filter(|_| any_unrunnable),
        })
    }

    /// The task this session works, or why `tool` has nothing to answer.
    fn task_access(&self, tool: RunTool) -> Result<&TaskAccess, RunToolError> {
        self.task
            .as_deref()
            .ok_or(RunToolError::NotATaskSession { tool: tool.name() })
    }
}

/// One task, as `yunta_task` answers it.
#[derive(Serialize)]
struct TaskSheet<'a> {
    id: &'a TaskId,
    title: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<&'a str>,
    /// What the task does and why, as the person who approved the plan
    /// read it.
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    #[serde(skip_serializing_if = "<[TaskId]>::is_empty")]
    depends_on: &'a [TaskId],
    /// What the diff is held to: declared plus granted.
    scope: &'a [ScopeGlob],
    criteria: Vec<Declared<'a>>,
    cycles: Vec<Cycle>,
    /// What the engine ran these checks with, told only when one of them
    /// could not run: the shell and the `PATH` its command was looked up in.
    #[serde(skip_serializing_if = "Option::is_none")]
    runs_under: Option<&'a ExecutionEnvironment>,
}

/// A criterion the task is judged by: one its document declares, or the
/// suite the run holds every task to — and what passing it shows, when
/// the plan or the run says so.
#[derive(Serialize)]
struct Declared<'a> {
    cmd: &'a str,
    guard: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    proves: Option<&'a str>,
}

/// A criterion and what one check of it answered.
#[derive(Serialize)]
struct Answered {
    cmd: String,
    guard: bool,
    exit_code: i32,
    /// Why the command never answered, when it did not: no work on the
    /// tree turns this green.
    #[serde(skip_serializing_if = "Option::is_none")]
    cannot_run: Option<&'static str>,
    /// The last lines a failing command printed: why it fails, without
    /// running it again.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tail: Vec<String>,
}

impl Answered {
    /// A check as the log holds it, its tail already redacted there.
    fn of_result(result: &CriterionResult) -> Self {
        Answered {
            cmd: result.cmd.clone(),
            guard: result.r#type == Some(CriterionType::Guard),
            exit_code: result.exit_code,
            cannot_run: could_not_run(result.exit_code),
            tail: result.tail.clone(),
        }
    }

    /// A check this call ran, its tail redacted the way the log would.
    fn of_run(run: &CriterionRun, redactor: &yunta_core::Redactor) -> Self {
        let tail = match (&run.output, run.exit_code) {
            (Some(output), exit_code) if exit_code != 0 => redactor
                .text(&output.tail().join("\n"))
                .lines()
                .map(str::to_string)
                .collect(),
            _ => Vec::new(),
        };
        Answered {
            cmd: run.cmd.clone(),
            guard: run.is_guard,
            exit_code: run.exit_code,
            cannot_run: run.could_not_run(),
            tail,
        }
    }
}

/// One cycle of the task: every check between the loop setting it
/// running and the next time it does.
#[derive(Serialize)]
struct Cycle {
    cycle: usize,
    checks: Vec<Check>,
}

/// One check of a cycle, as the log holds it.
#[derive(Serialize)]
struct Check {
    phase: Phase,
    /// Which attempt of its cycle a post-check closed; a pre-check
    /// belongs to none.
    #[serde(skip_serializing_if = "Option::is_none")]
    attempt: Option<usize>,
    criteria: Vec<Answered>,
    /// What that attempt changed outside the task's scope.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    outside_scope: Vec<PathBuf>,
}

/// What `yunta_check_task` answers.
#[derive(Serialize)]
struct Verdict<'a> {
    /// Whether the task would be done if the session ended now.
    closes: bool,
    criteria: Vec<Answered>,
    outside_scope: Vec<PathBuf>,
    /// What the criteria ran with, told only when one could not run.
    #[serde(skip_serializing_if = "Option::is_none")]
    runs_under: Option<&'a ExecutionEnvironment>,
}

/// Every cycle `task` has run, oldest first, as the log holds them: a
/// cycle begins each time the loop sets the task running, and holds its
/// pre-check, then each attempt's post-check carrying what that
/// attempt's scope audit found outside the scope. The last cycle is the
/// one under way; the ones before it are what earlier attempts — a
/// retried loop's among them — already learned.
fn cycles_of(events: &[StoredEvent], task: &TaskId) -> Vec<Cycle> {
    let mut cycles: Vec<Cycle> = Vec::new();
    for event in events {
        match event.payload() {
            Some(EventPayload::Tasks(TaskEvent::StatusChanged(p)))
                if p.task_id == *task && p.new_status == TaskStatus::Running =>
            {
                cycles.push(Cycle {
                    cycle: cycles.len() + 1,
                    checks: Vec::new(),
                });
            }
            Some(EventPayload::Node(NodeEvent::CriteriaChecked(p))) if p.task_id == *task => {
                let Some(cycle) = cycles.last_mut() else {
                    continue;
                };
                let attempt = match p.phase {
                    Phase::Pre => None,
                    Phase::Post => Some(
                        cycle
                            .checks
                            .iter()
                            .filter(|check| check.phase == Phase::Post)
                            .count()
                            + 1,
                    ),
                };
                cycle.checks.push(Check {
                    phase: p.phase,
                    attempt,
                    criteria: p.results.iter().map(Answered::of_result).collect(),
                    outside_scope: Vec::new(),
                });
            }
            Some(EventPayload::Node(NodeEvent::ScopeChecked(p)))
                if p.task_id.as_ref() == Some(task) =>
            {
                if let Some(check) = cycles.last_mut().and_then(|cycle| cycle.checks.last_mut()) {
                    check.outside_scope = p.violations.clone();
                }
            }
            _ => {}
        }
    }
    cycles
}

pub(super) fn render(answer: &impl Serialize) -> Result<String, RunToolError> {
    serde_json::to_string_pretty(answer).map_err(|source| RunToolError::Render { source })
}
