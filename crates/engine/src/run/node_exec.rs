//! Executing one node (T4.1 recorte) — the imperative half. Every
//! outcome, good or bad, lands in the event log; a node that cannot run
//! (undefined template variable, unresolvable runner, unsupported
//! `until`) fails *in the log* with a diagnostic, it does not abort the
//! engine — degradación explícita, jamás silenciosa.

use std::collections::BTreeMap;

use yunta_adapters::{Budget, PermissionProfile, SessionRequest};
use yunta_core::events::{
    CriterionResult, CriterionType, EventPayload, HookExecutedPayload, HookPhase,
    LoopIterationPayload, NodeFailedPayload, NodeFinishedPayload, Phase, RunnerResolvedPayload,
    TaskStatus, TokenUsage,
};
use yunta_core::{Ledger, Node, NodeKind, PromptSource, Task};

use crate::artifacts::close_artifacts;
use crate::replay::derive;
use crate::runner::resolve_runner;
use crate::scope::scope_check;
use crate::task_cycle::{dispatch_session, run_task, DispatchOutcome, TaskOutcome};
use crate::template::render_template;

use super::{RunCtx, RunError};

/// How the node's execution ended, as recorded in the log by the caller.
pub(super) enum NodeEnd {
    Finished,
    Failed,
}

pub(super) async fn execute_node(
    ctx: &RunCtx<'_>,
    node: &Node,
    attempt: u32,
) -> Result<NodeEnd, RunError> {
    ctx.emit(
        Some(&node.id),
        EventPayload::NodeStarted(yunta_core::events::NodeStartedPayload { attempt }),
    )?;

    // hooks.before (§11.1): a failing before aborts without spending a
    // token; a failing after fails the node before verification.
    if let Some(hooks) = &node.hooks {
        for step in &hooks.before {
            if !run_hook(ctx, node, HookPhase::Before, &step.run).await? {
                return fail(
                    ctx,
                    node,
                    format!("before hook `{}` failed", step.run),
                    false,
                );
            }
        }
    }

    let end = match &node.kind {
        NodeKind::Bash { run } => execute_bash(ctx, node, run).await?,
        NodeKind::Prompt { prompt } => execute_prompt(ctx, node, prompt).await?,
        NodeKind::Loop { until, prompt } => execute_loop(ctx, node, until, prompt).await?,
    };
    Ok(end)
}

fn template_vars(ctx: &RunCtx<'_>) -> BTreeMap<String, String> {
    BTreeMap::from([("run.dir".to_string(), ctx.run_dir.display().to_string())])
}

/// Renders `input` or fails the node with a diagnostic naming the
/// variable — a prompt with `{{run.dir}}` left verbatim must never reach
/// an agent.
fn render_or_fail(
    ctx: &RunCtx<'_>,
    node: &Node,
    input: &str,
) -> Result<Result<String, NodeEnd>, RunError> {
    match render_template(input, &template_vars(ctx)) {
        Ok(rendered) => Ok(Ok(rendered)),
        Err(e) => {
            let end = fail(ctx, node, e.to_string(), false)?;
            Ok(Err(end))
        }
    }
}

async fn run_hook(
    ctx: &RunCtx<'_>,
    node: &Node,
    phase: HookPhase,
    command: &str,
) -> Result<bool, RunError> {
    let rendered = match render_template(command, &template_vars(ctx)) {
        Ok(rendered) => rendered,
        Err(e) => {
            // An unrenderable hook is a failed hook — recorded as such.
            ctx.emit(
                Some(&node.id),
                EventPayload::HookExecuted(HookExecutedPayload {
                    phase,
                    command: command.to_string(),
                    exit_code: -1,
                }),
            )?;
            tracing::warn!(node_id = %node.id, error = %e, "hook template failed to render");
            return Ok(false);
        }
    };
    let status = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&rendered)
        .current_dir(ctx.worktree)
        .status()
        .await
        .map_err(|source| RunError::Io {
            context: format!("run hook `{rendered}`"),
            source,
        })?;
    let exit_code = status.code().unwrap_or(-1);
    ctx.emit(
        Some(&node.id),
        EventPayload::HookExecuted(HookExecutedPayload {
            phase,
            command: rendered,
            exit_code,
        }),
    )?;
    Ok(exit_code == 0)
}

/// Runs after-hooks, then verifies scope and artifacts — the close
/// sequence every successful node body goes through (§11.1's order:
/// session → after → verificación).
async fn close_node(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: String,
    tokens: TokenUsage,
) -> Result<NodeEnd, RunError> {
    if let Some(hooks) = &node.hooks {
        for step in &hooks.after {
            if !run_hook(ctx, node, HookPhase::After, &step.run).await? {
                return fail_with_tokens(
                    ctx,
                    node,
                    format!("after hook `{}` failed", step.run),
                    false,
                    tokens,
                );
            }
        }
    }

    // Scope check over the node's whole diff (§6) — hook edits included.
    if !node.scope.is_empty() {
        let result = scope_check(ctx.worktree, &node.scope).await?;
        ctx.emit(
            Some(&node.id),
            EventPayload::ScopeChecked(yunta_core::events::ScopeCheckedPayload {
                task_id: None,
                diff: result.diff.clone(),
                violations: result.violations.clone(),
            }),
        )?;
        if !result.violations.is_empty() {
            return fail_with_tokens(
                ctx,
                node,
                format!(
                    "scope violated: {} file(s) outside the declared globs",
                    result.violations.len()
                ),
                false,
                tokens,
            );
        }
    }

    match close_artifacts(node, ctx.run_dir) {
        Ok(verified) => {
            for artifact in &verified {
                ctx.emit(
                    Some(&node.id),
                    EventPayload::ArtifactWritten(yunta_core::events::ArtifactWrittenPayload {
                        path: artifact.path.clone(),
                        content_hash: artifact.content_hash.clone(),
                    }),
                )?;
                if let Some(ledger) = &artifact.ledger {
                    for task in &ledger.tasks {
                        ctx.emit(
                            Some(&node.id),
                            EventPayload::TaskRegistered(
                                yunta_core::events::TaskRegisteredPayload {
                                    task_id: task.id.clone(),
                                    criteria: task.criteria.clone(),
                                    scope: task.scope.clone(),
                                    depends_on: task.depends_on.clone(),
                                },
                            ),
                        )?;
                    }
                }
            }
            ctx.emit(
                Some(&node.id),
                EventPayload::NodeFinished(NodeFinishedPayload {
                    outcome,
                    tokens_used: tokens,
                }),
            )?;
            Ok(NodeEnd::Finished)
        }
        Err(errors) => {
            let listed = errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            fail_with_tokens(ctx, node, listed, false, tokens)
        }
    }
}

fn fail(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: String,
    retryable: bool,
) -> Result<NodeEnd, RunError> {
    fail_with_tokens(ctx, node, outcome, retryable, TokenUsage::default())
}

fn fail_with_tokens(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: String,
    retryable: bool,
    tokens: TokenUsage,
) -> Result<NodeEnd, RunError> {
    ctx.emit(
        Some(&node.id),
        EventPayload::NodeFailed(NodeFailedPayload {
            outcome,
            tokens_used: tokens,
            retryable,
        }),
    )?;
    Ok(NodeEnd::Failed)
}

async fn execute_bash(ctx: &RunCtx<'_>, node: &Node, run: &str) -> Result<NodeEnd, RunError> {
    let rendered = match render_or_fail(ctx, node, run)? {
        Ok(rendered) => rendered,
        Err(end) => return Ok(end),
    };

    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&rendered)
        .current_dir(ctx.worktree)
        .output()
        .await
        .map_err(|source| RunError::Io {
            context: format!("run bash node `{}`", node.id),
            source,
        })?;

    if output.status.success() {
        close_node(ctx, node, "exit 0".to_string(), TokenUsage::default()).await
    } else {
        let stderr_tail: String = String::from_utf8_lossy(&output.stderr)
            .lines()
            .rev()
            .take(20)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        fail(
            ctx,
            node,
            format!("exit {}: {stderr_tail}", output.status.code().unwrap_or(-1)),
            false,
        )
    }
}

/// The node's prompt text: frozen file content from the manifest when the
/// workflow declared `{file: ...}`, the inline string otherwise — never a
/// re-read from disk (§2.1).
fn prompt_text<'a>(ctx: &'a RunCtx<'_>, node: &'a Node, prompt: &'a PromptSource) -> &'a str {
    match prompt {
        PromptSource::Inline(text) => text,
        PromptSource::File(_) => ctx
            .manifest
            .prompts
            .get(&node.id)
            .map(String::as_str)
            // A file prompt missing from the manifest cannot happen for a
            // manifest built by `build_manifest` (it reads every file
            // prompt or errors); an empty prompt for a hand-edited
            // manifest fails the session visibly downstream.
            .unwrap_or(""),
    }
}

/// Resolves the node's runner or fails the node; on success emits
/// `runner_resolved` and hands back the request pieces.
fn resolve_node_runner(
    ctx: &RunCtx<'_>,
    node: &Node,
) -> Result<Result<yunta_core::RunnerCandidate, NodeEnd>, RunError> {
    let Some(role) = &node.runner else {
        let end = fail(
            ctx,
            node,
            format!(
                "node `{}` has no `runner:` and M-0 has no `defaults.runner` — declare one",
                node.id
            ),
            false,
        )?;
        return Ok(Err(end));
    };

    match resolve_runner(role, &ctx.manifest.config, &|adapter| {
        ctx.adapters.contains_key(adapter)
    }) {
        Ok(resolved) => {
            ctx.emit(
                Some(&node.id),
                EventPayload::RunnerResolved(RunnerResolvedPayload {
                    role: resolved.role.clone(),
                    chosen: resolved.chosen.clone(),
                    discarded: resolved.discarded.clone(),
                }),
            )?;
            Ok(Ok(resolved.chosen))
        }
        Err(e) => {
            let end = fail(ctx, node, e.to_string(), false)?;
            Ok(Err(end))
        }
    }
}

async fn execute_prompt(
    ctx: &RunCtx<'_>,
    node: &Node,
    prompt: &PromptSource,
) -> Result<NodeEnd, RunError> {
    let rendered = match render_or_fail(ctx, node, prompt_text(ctx, node, prompt))? {
        Ok(rendered) => rendered,
        Err(end) => return Ok(end),
    };
    let chosen = match resolve_node_runner(ctx, node)? {
        Ok(chosen) => chosen,
        Err(end) => return Ok(end),
    };

    let adapter = &ctx.adapters[&chosen.adapter];
    let request = SessionRequest {
        prompt: rendered,
        cwd: ctx.worktree.to_path_buf(),
        model: Some(chosen.model),
        agent: chosen.agent,
        permissions: PermissionProfile::Edit,
        env: Default::default(),
        edit_constraints: (!node.scope.is_empty()).then(|| node.scope.clone()),
        budget: Budget::default(),
        adapter_settings: Default::default(),
    };

    let (outcome, tokens) =
        dispatch_session(adapter.as_ref(), request)
            .await
            .map_err(|source| RunError::Spawn {
                node: node.id.clone(),
                source,
            })?;

    match outcome {
        DispatchOutcome::Completed { summary } => close_node(ctx, node, summary, tokens).await,
        DispatchOutcome::Failed { message, retryable } => {
            fail_with_tokens(ctx, node, message, retryable, tokens)
        }
        // O2: no terminal event means the engine synthesizes a retryable
        // failure — the adapter never invents one.
        DispatchOutcome::Crashed => fail_with_tokens(
            ctx,
            node,
            "session ended without a terminal event".to_string(),
            true,
            tokens,
        ),
        DispatchOutcome::BudgetExceeded { reason } => {
            fail_with_tokens(ctx, node, reason, false, tokens)
        }
    }
}

async fn execute_loop(
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
            reused: false, // memoization is T5.9, out of M-0
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
            let yunta_core::ArtifactKind::TaskLedger = kind;
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
