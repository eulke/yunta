//! Executing one node (T4.1 recorte) — the imperative half. Every
//! outcome, good or bad, lands in the event log; a node that cannot run
//! (undefined template variable, unresolvable runner, unsupported
//! `until`) fails *in the log* with a diagnostic, it does not abort the
//! engine — degradación explícita, jamás silenciosa.

use std::collections::BTreeMap;

use yunta_adapters::{Budget, PermissionProfile, SessionRequest};
use yunta_core::events::{
    EventPayload, HookExecutedPayload, HookPhase, NodeFailedPayload, NodeFinishedPayload,
    RunnerResolvedPayload, TokenUsage,
};
use yunta_core::{Node, NodeKind, PromptSource};

use crate::artifacts::close_artifacts;
use crate::runner::resolve_runner;
use crate::scope::scope_check;
use crate::task_cycle::{dispatch_session, DispatchOutcome};
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
        NodeKind::Loop { until, prompt } => {
            super::loop_exec::execute_loop(ctx, node, until, prompt).await?
        }
    };
    Ok(end)
}

fn template_vars(ctx: &RunCtx<'_>) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("run.dir".to_string(), ctx.run_dir.display().to_string()),
        (
            "run.worktree".to_string(),
            ctx.worktree.display().to_string(),
        ),
    ])
}

/// Renders `input` or fails the node with a diagnostic naming the
/// variable — a prompt with `{{run.dir}}` left verbatim must never reach
/// an agent.
pub(super) fn render_or_fail(
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
pub(super) async fn close_node(
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

pub(super) fn fail(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: String,
    retryable: bool,
) -> Result<NodeEnd, RunError> {
    fail_with_tokens(ctx, node, outcome, retryable, TokenUsage::default())
}

pub(super) fn fail_with_tokens(
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
pub(super) fn prompt_text<'a>(
    ctx: &'a RunCtx<'_>,
    node: &'a Node,
    prompt: &'a PromptSource,
) -> &'a str {
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
pub(super) fn resolve_node_runner(
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
