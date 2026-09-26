//! The tools that speak about the run's tasks document: where its tasks
//! stand, what one task session's task is and how its work stands, and
//! how that session asks for its task's scope to be widened.
//!
//! None of them writes the document or a task's status. A task session
//! reads its task here because this is the only place it can: the
//! document lives with the run, not in the checkout, and a copy of it in
//! the session's prompt would be a second source that could disagree
//! with the one the engine judges by. What `yunta_task` answers is the
//! task the cycle holds and the checks the log holds; what
//! `yunta_check_task` answers is the judgement the attempt's close makes.
//!
//! A session never widens its own scope either: the request is written
//! as the very file the file-based path produces, so the engine's
//! existing post-attempt evaluation (rules/ask/deny, cap, findings on
//! denial) decides it — one mechanism with two intake surfaces, rather
//! than a second policy living behind the listener.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;
use yunta_core::events::{
    CriterionResult, CriterionType, EventPayload, NodeEvent, Phase, StoredEvent, TaskEvent,
    TaskStatus,
};
use yunta_core::{ScopeGlob, TaskId};

use super::catalog::RunTool;
use super::host::TaskAccess;
use super::session::{RunToolError, SessionTools};
use crate::task_cycle::{judge, CriterionRun, Work};

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
            depends_on: &task.depends_on,
            scope: &access.scope,
            criteria: task
                .criteria
                .iter()
                .map(|criterion| Declared {
                    cmd: &criterion.cmd,
                    guard: criterion.r#type == Some(CriterionType::Guard),
                })
                .collect(),
            checks: checks_of(&events, &task.id),
        };
        render(&sheet)
    }

    /// The judgement this session's attempt would get if it ended now.
    pub(super) async fn check_task(&self) -> Result<String, RunToolError> {
        let access = self.task_access(RunTool::CheckTask)?;
        let judgement = judge(
            &access.task,
            &access.scope,
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
        render(&Verdict {
            closes: judgement.closes(),
            criteria: judgement.criteria.iter().map(Answered::of_run).collect(),
            outside_scope: judgement.scope.violations,
        })
    }

    /// The task this session works, or why `tool` has nothing to answer.
    fn task_access(&self, tool: RunTool) -> Result<&TaskAccess, RunToolError> {
        self.task
            .as_deref()
            .ok_or(RunToolError::NotATaskSession { tool: tool.name() })
    }

    pub(super) async fn request_scope_expansion(
        &self,
        args: serde_json::Map<String, Value>,
    ) -> Result<String, RunToolError> {
        if self.task.is_none() {
            return Err(RunToolError::NoTask);
        }
        // The identical request object the file-based path uses,
        // validated by the same
        // type it parses — then written as that exact
        // file, so the engine's existing post-attempt evaluation
        // (rules/ask/deny, cap, findings on denial) consumes it
        // unchanged: one mechanism, two intake surfaces.
        let request: crate::scope_expansion::ScopeExpansionRequest =
            serde_json::from_value(Value::Object(args))
                .map_err(|source| RunToolError::InvalidRequest { source })?;
        let path = self
            .cwd
            .join(crate::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE);
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Err(RunToolError::RequestPending);
        }
        let yaml = yunta_core::yaml::to_string(&request)
            .map_err(|source| RunToolError::Yaml { source })?;
        tokio::fs::write(&path, yaml)
            .await
            .map_err(|source| RunToolError::Write {
                path: path.clone(),
                source,
            })?;
        Ok(
            "request recorded — it is evaluated when this attempt ends (the engine \
             or a person decides; a denial becomes a finding); re-attempt the work after"
                .to_string(),
        )
    }
}

/// One task, as `yunta_task` answers it.
#[derive(Serialize)]
struct TaskSheet<'a> {
    id: &'a TaskId,
    title: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<&'a str>,
    #[serde(skip_serializing_if = "<[TaskId]>::is_empty")]
    depends_on: &'a [TaskId],
    /// What the diff is held to: declared plus granted.
    scope: &'a [ScopeGlob],
    criteria: Vec<Declared<'a>>,
    checks: Vec<Check>,
}

/// A criterion as the task declares it.
#[derive(Serialize)]
struct Declared<'a> {
    cmd: &'a str,
    guard: bool,
}

/// A criterion and what one check of it answered.
#[derive(Serialize)]
struct Answered {
    cmd: String,
    guard: bool,
    exit_code: i32,
}

impl Answered {
    fn of_result(result: &CriterionResult) -> Self {
        Answered {
            cmd: result.cmd.clone(),
            guard: result.r#type == Some(CriterionType::Guard),
            exit_code: result.exit_code,
        }
    }

    fn of_run(run: &CriterionRun) -> Self {
        Answered {
            cmd: run.cmd.clone(),
            guard: run.is_guard,
            exit_code: run.exit_code,
        }
    }
}

/// One check of the task's current cycle, as the log holds it.
#[derive(Serialize)]
struct Check {
    phase: Phase,
    /// Which attempt a post-check closed; a pre-check belongs to none.
    #[serde(skip_serializing_if = "Option::is_none")]
    attempt: Option<u32>,
    criteria: Vec<Answered>,
    /// What that attempt changed outside the task's scope.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    outside_scope: Vec<PathBuf>,
}

/// What `yunta_check_task` answers.
#[derive(Serialize)]
struct Verdict {
    /// Whether the task would be done if the session ended now.
    closes: bool,
    criteria: Vec<Answered>,
    outside_scope: Vec<PathBuf>,
}

/// `task`'s checks in its current cycle — everything after the last time
/// the loop set it running — as the log holds them: the pre-check, then
/// each attempt's post-check carrying what that attempt's scope audit
/// found outside the scope.
fn checks_of(events: &[StoredEvent], task: &TaskId) -> Vec<Check> {
    let start = events
        .iter()
        .rposition(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Tasks(TaskEvent::StatusChanged(p)))
                    if p.task_id == *task && p.new_status == TaskStatus::Running
            )
        })
        .map_or(0, |at| at + 1);
    let mut checks: Vec<Check> = Vec::new();
    let mut attempts = 0;
    for event in events.iter().skip(start) {
        match event.payload() {
            Some(EventPayload::Node(NodeEvent::CriteriaChecked(p))) if p.task_id == *task => {
                let attempt = match p.phase {
                    Phase::Pre => None,
                    Phase::Post => {
                        attempts += 1;
                        Some(attempts)
                    }
                };
                checks.push(Check {
                    phase: p.phase,
                    attempt,
                    criteria: p.results.iter().map(Answered::of_result).collect(),
                    outside_scope: Vec::new(),
                });
            }
            Some(EventPayload::Node(NodeEvent::ScopeChecked(p)))
                if p.task_id.as_ref() == Some(task) =>
            {
                if let Some(check) = checks.last_mut() {
                    check.outside_scope = p.violations.clone();
                }
            }
            _ => {}
        }
    }
    checks
}

fn render(answer: &impl Serialize) -> Result<String, RunToolError> {
    serde_json::to_string_pretty(answer).map_err(|source| RunToolError::Render { source })
}
