//! The loop node's task cycle (T4.1 + T5.2 wiring, Contrato §5.2/§5.5):
//! iterate the registered ledger task by task, run each through
//! [`run_task`], record every check in the log, and commit verified work
//! so the next task's scope check sees only its own diff.

use yunta_adapters::Budget;
use yunta_core::events::{
    CriterionResult, CriterionType, EventPayload, LoopIterationPayload, Phase, TaskStatus,
    TokenUsage,
};
use yunta_core::{Ledger, Node, PromptSource, Task};

use crate::replay::derive;
use crate::task_cycle::{run_task, TaskOutcome};

use super::node_exec::{
    close_node, fail, fail_with_tokens, prompt_text, render_or_fail, resolve_node_runner, NodeEnd,
};
use super::{RunCtx, RunError};

pub(super) async fn execute_loop(
    ctx: &RunCtx<'_>,
    node: &Node,
    until: &str,
    prompt: &PromptSource,
) -> Result<NodeEnd, RunError> {
    if until != "all_tasks_complete" {
        return fail(
            ctx,
            node,
            format!("loop until `{until}` is not supported — M-0 only has `all_tasks_complete`"),
            false,
        );
    }
    let instruction = match render_or_fail(ctx, node, prompt_text(ctx, node, prompt))? {
        Ok(rendered) => rendered,
        Err(end) => return Ok(end),
    };
    let chosen = match resolve_node_runner(ctx, node)? {
        Ok(chosen) => chosen,
        Err(end) => return Ok(end),
    };
    let adapter = &ctx.adapters[&chosen.adapter];

    let Some(ledger) = load_registered_ledger(ctx)? else {
        return fail(
            ctx,
            node,
            "no task ledger has been registered before this loop — a previous node must \
             produce an artifact with `kind: task-ledger`"
                .to_string(),
            false,
        );
    };

    let mut tokens = TokenUsage::default();
    let mut iteration: u32 = 0;

    loop {
        iteration += 1;
        let events = ctx.load_events()?;
        let state = derive(&events);

        let next_task = ledger.tasks.iter().find(|task| {
            state.tasks.get(&task.id) == Some(&TaskStatus::Pending)
                && task
                    .depends_on
                    .iter()
                    .all(|dep| state.tasks.get(dep) == Some(&TaskStatus::Done))
        });

        let Some(task) = next_task else {
            let all_done = ledger
                .tasks
                .iter()
                .all(|task| state.tasks.get(&task.id) == Some(&TaskStatus::Done));
            ctx.emit(
                Some(&node.id),
                EventPayload::LoopIteration(LoopIterationPayload {
                    iteration,
                    until_result: all_done,
                }),
            )?;
            if all_done {
                return close_node(
                    ctx,
                    node,
                    format!("{} task(s) done", ledger.tasks.len()),
                    tokens,
                )
                .await;
            }
            return fail_with_tokens(
                ctx,
                node,
                "no task is ready and not all are done — blocked or failed tasks need a decision"
                    .to_string(),
                false,
                tokens,
            );
        };

        let registered_seq = events
            .iter()
            .find(|event| {
                matches!(&event.payload, EventPayload::TaskRegistered(p) if p.task_id == task.id)
            })
            .map(|event| event.seq)
            .unwrap_or(0);
        run_one_task(
            ctx,
            node,
            task,
            registered_seq,
            &instruction,
            adapter.as_ref(),
            &mut tokens,
        )
        .await?;
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_one_task(
    ctx: &RunCtx<'_>,
    node: &Node,
    task: &Task,
    registered_seq: u64,
    instruction: &str,
    adapter: &dyn yunta_adapters::Adapter,
    tokens: &mut TokenUsage,
) -> Result<(), RunError> {
    ctx.emit(
        Some(&node.id),
        EventPayload::TaskStatusChanged(yunta_core::events::TaskStatusChangedPayload {
            task_id: task.id.clone(),
            new_status: TaskStatus::Running,
            caused_by: registered_seq,
        }),
    )?;

    let report = run_task(
        task,
        instruction,
        adapter,
        ctx.worktree,
        ctx.max_task_retries,
        Budget::default(),
        &ctx.memo,
    )
    .await?;

    let mut last_check_seq = ctx.emit(
        Some(&node.id),
        EventPayload::CriteriaChecked(yunta_core::events::CriteriaCheckedPayload {
            task_id: task.id.clone(),
            phase: Phase::Pre,
            results: to_results(&report.pre_check),
        }),
    )?;

    for attempt in &report.attempts {
        *tokens = sum_tokens(*tokens, attempt.tokens);
        last_check_seq = ctx.emit(
            Some(&node.id),
            EventPayload::CriteriaChecked(yunta_core::events::CriteriaCheckedPayload {
                task_id: task.id.clone(),
                phase: Phase::Post,
                results: to_results(&attempt.post_check),
            }),
        )?;
        ctx.emit(
            Some(&node.id),
            EventPayload::ScopeChecked(yunta_core::events::ScopeCheckedPayload {
                task_id: Some(task.id.clone()),
                diff: attempt.scope.diff.clone(),
                violations: attempt.scope.violations.clone(),
            }),
        )?;
    }

    let new_status = match &report.outcome {
        TaskOutcome::Done => TaskStatus::Done,
        TaskOutcome::Blocked { .. } => TaskStatus::Blocked,
    };
    if new_status == TaskStatus::Done {
        // §5.5: a verified task is committed before the next one runs, so
        // every task's scope check sees only its own diff — without this,
        // T001's uncommitted edits would count against T002's scope.
        commit_task_work(ctx, task).await?;
    }
    ctx.emit(
        Some(&node.id),
        EventPayload::TaskStatusChanged(yunta_core::events::TaskStatusChangedPayload {
            task_id: task.id.clone(),
            new_status,
            caused_by: last_check_seq,
        }),
    )?;
    Ok(())
}

/// Commits a done task's work in the worktree. A task that changed
/// nothing (its criteria were satisfied by side effects that left no
/// diff) simply produces no commit — never an error.
async fn commit_task_work(ctx: &RunCtx<'_>, task: &Task) -> Result<(), RunError> {
    let git = |args: Vec<String>| {
        let worktree = ctx.worktree.to_path_buf();
        async move {
            tokio::process::Command::new("git")
                .args(&args)
                .current_dir(&worktree)
                .output()
                .await
        }
    };

    let add = git(vec!["add".into(), "-A".into()])
        .await
        .map_err(|source| RunError::Io {
            context: format!("stage task `{}` work", task.id),
            source,
        })?;
    if !add.status.success() {
        return Err(RunError::Git {
            context: format!("stage task `{}` work", task.id),
            detail: String::from_utf8_lossy(&add.stderr).trim().to_string(),
        });
    }

    let staged = git(vec!["diff".into(), "--cached".into(), "--quiet".into()])
        .await
        .map_err(|source| RunError::Io {
            context: format!("inspect staged work for task `{}`", task.id),
            source,
        })?;
    if staged.status.success() {
        return Ok(()); // nothing staged — nothing to commit
    }

    let commit = git(vec![
        "commit".into(),
        "-q".into(),
        "-m".into(),
        format!("task {}: {}", task.id, task.title),
    ])
    .await
    .map_err(|source| RunError::Io {
        context: format!("commit task `{}` work", task.id),
        source,
    })?;
    if !commit.status.success() {
        return Err(RunError::Git {
            context: format!("commit task `{}` work", task.id),
            detail: String::from_utf8_lossy(&commit.stderr).trim().to_string(),
        });
    }
    Ok(())
}

fn to_results(runs: &[crate::task_cycle::CriterionRun]) -> Vec<CriterionResult> {
    runs.iter()
        .map(|run| CriterionResult {
            cmd: run.cmd.clone(),
            exit_code: run.exit_code,
            r#type: run.is_guard.then_some(CriterionType::Guard),
            reused: run.reused,
        })
        .collect()
}

fn sum_tokens(a: TokenUsage, b: TokenUsage) -> TokenUsage {
    TokenUsage {
        input: a.input + b.input,
        output: a.output + b.output,
        cached: match (a.cached, b.cached) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        },
    }
}

/// Finds the task ledger the run registered: the `kind: task-ledger`
/// artifact of a node that produced it earlier, re-read from the run's
/// frozen `artifacts/` (I3: artifacts are immutable once written).
fn load_registered_ledger(ctx: &RunCtx<'_>) -> Result<Option<Ledger>, RunError> {
    for node in &ctx.manifest.workflow.nodes {
        let Some(artifacts) = &node.artifacts else {
            continue;
        };
        for spec in &artifacts.produces {
            let yunta_core::ArtifactSpec::Typed { name, kind } = spec else {
                continue;
            };
            if !matches!(kind, yunta_core::ArtifactKind::TaskLedger) {
                continue;
            }
            let path = ctx.run_dir.join("artifacts").join(name);
            if !path.exists() {
                continue;
            }
            let bytes = std::fs::read(&path).map_err(|source| RunError::Io {
                context: format!("read task ledger `{}`", path.display()),
                source,
            })?;
            let ledger: Ledger =
                serde_yaml::from_slice(&bytes).map_err(|e| RunError::CorruptLedger {
                    path: path.clone(),
                    detail: e.to_string(),
                })?;
            return Ok(Some(ledger));
        }
    }
    Ok(None)
}
