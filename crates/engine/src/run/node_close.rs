//! How a node ends: closing its artifacts and staged paths on success,
//! recording a failure, and refreshing `progress.md` either way.

use std::collections::BTreeMap;

use yunta_core::events::{
    EventPayload, HookPhase, NodeFailedPayload, NodeFinishedPayload, TaskStatus,
    TaskStatusChangedPayload, TokenUsage,
};
use yunta_core::{HookFailurePolicy, Node};

use crate::artifacts::close_artifacts;
use crate::scope::scope_check;

use super::hooks_exec::{effective_hooks, run_hook, HookRun};
use super::node_exec::{render_artifact_names, NodeEnd};
use super::{RunCtx, RunError};

/// Runs after-hooks, then verifies scope and artifacts — the close
/// sequence every successful node body goes through (session → after →
/// verification).
/// Closes a node whose session staged nothing in the worktree — every
/// kind but an agent session.
pub(super) async fn close_node(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: String,
    tokens: TokenUsage,
) -> Result<NodeEnd, RunError> {
    close_node_staged(ctx, node, outcome, tokens, &[]).await
}

/// Closes a node: its `after` hooks run, its scope is checked over the
/// whole diff — hook edits included, `staged` paths (what the adapter
/// declared it wrote for itself) left out — and it finishes.
pub(super) async fn close_node_staged(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: String,
    tokens: TokenUsage,
    staged: &[std::path::PathBuf],
) -> Result<NodeEnd, RunError> {
    for step in &effective_hooks(ctx, node).after {
        match run_hook(ctx, node, HookPhase::After, step).await? {
            HookRun::Violation(rule) => {
                return fail_with_tokens(ctx, node, rule, false, tokens).await
            }
            HookRun::Ran(false) if step.on_failure == HookFailurePolicy::Fail => {
                return fail_with_tokens(
                    ctx,
                    node,
                    format!("after hook `{}` failed", step.run),
                    false,
                    tokens,
                )
                .await;
            }
            HookRun::Ran(_) => {}
        }
    }

    if !node.scope.is_empty() {
        let result = scope_check(ctx.worktree, &node.scope, staged).await?;
        ctx.emit(
            Some(&node.id),
            EventPayload::ScopeChecked(yunta_core::events::ScopeCheckedPayload {
                task_id: None,
                diff: result.diff.clone(),
                violations: result.violations.clone(),
            }),
        )
        .await?;
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
            )
            .await;
        }
    }

    let max_artifact_bytes = ctx
        .manifest
        .config
        .limits
        .as_ref()
        .and_then(|limits| limits.max_artifact_bytes);
    // Artifact names are templates too (`findings-{{runner.role}}`
    // in the reference workflow) — rendered per node so every fan-out
    // sibling verifies its own file.
    let node_rendered = match render_artifact_names(ctx, node) {
        Ok(rendered) => rendered,
        Err(error) => return fail_with_tokens(ctx, node, error.to_string(), false, tokens).await,
    };
    let node = &node_rendered;
    match close_artifacts(node, ctx.run_dir, max_artifact_bytes) {
        Ok(verified) => {
            // A `kind: questions` artifact's own
            // session has already closed by this point (the same
            // "artifact read only at node close" ordering
            // `task-ledger`/`findings` already rely on) — nothing
            // renders mid-session. With a live surface the questions
            // are put to the human right here (after the artifacts are
            // recorded, below); without one — or on an invalid reply —
            // the run pauses citing exactly what's unanswered, never
            // hangs, never silently proceeds as if nothing were asked.

            // A re-plan — this same node producing a task
            // ledger a second time, whether via a reroute back to it or a
            // resumed run — must not silently keep a task `done` whose
            // identity actually changed. Identity is same `id`, same
            // `criteria`, same `scope` — `depends_on` is deliberately not
            // part of it. The most recent prior registration
            // per task id is all that's needed; `TaskRegistered`'s own
            // replay handling (`or_insert`, never overwrites an existing
            // status) already makes an identical re-registration a no-op,
            // so only a genuine mismatch needs an explicit event here.
            let previous_registrations: BTreeMap<
                yunta_core::TaskId,
                (Vec<yunta_core::events::Criterion>, Vec<String>),
            > = ctx
                .load_events()
                .await?
                .into_iter()
                .filter_map(|event| match event.payload() {
                    Some(EventPayload::TaskRegistered(p)) => {
                        Some((p.task_id.clone(), (p.criteria.clone(), p.scope.clone())))
                    }
                    _ => None,
                })
                .collect();

            for artifact in &verified {
                ctx.emit(
                    Some(&node.id),
                    EventPayload::ArtifactWritten(yunta_core::events::ArtifactWrittenPayload {
                        path: artifact.path.clone(),
                        content_hash: artifact.content_hash.clone(),
                        artifact_kind: artifact.kind.clone(),
                    }),
                )
                .await?;
                if let Some(ledger) = &artifact.ledger {
                    for task in &ledger.tasks {
                        let criteria: Vec<yunta_core::events::Criterion> =
                            task.criteria.iter().map(Into::into).collect();
                        let registered_seq = ctx
                            .emit(
                                Some(&node.id),
                                EventPayload::TaskRegistered(
                                    yunta_core::events::TaskRegisteredPayload {
                                        task_id: task.id.clone(),
                                        criteria: criteria.clone(),
                                        scope: task.scope.clone(),
                                        depends_on: task.depends_on.clone(),
                                    },
                                ),
                            )
                            .await?;
                        let changed_identity = previous_registrations.get(&task.id).is_some_and(
                            |(previous, scope)| *previous != criteria || *scope != task.scope,
                        );
                        if changed_identity {
                            ctx.emit(
                                Some(&node.id),
                                EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
                                    task_id: task.id.clone(),
                                    new_status: TaskStatus::Pending,
                                    caused_by: registered_seq,
                                }),
                            )
                            .await?;
                        }
                    }
                }
                if let Some(findings) = &artifact.findings {
                    for finding in findings {
                        ctx.emit(
                            Some(&node.id),
                            EventPayload::FindingPosted(yunta_core::events::FindingPostedPayload {
                                finding: finding.clone(),
                            }),
                        )
                        .await?;
                    }
                }
            }
            // Unanswered questions close the node as
            // waiting-shaped (`node_failed` here, derived `Waiting` by
            // replay via the typed `kind: questions` on the artifact
            // event above) — the actual asking happens in ONE place, the
            // scheduler's own `AskQuestions` step (`questions_exec`),
            // which serves the first invocation and every resume through
            // the identical path.
            let pending: Vec<String> = verified
                .iter()
                .filter_map(|artifact| artifact.questions.as_deref())
                .flatten()
                .map(|q| q.id.to_string())
                .collect();
            if !pending.is_empty() {
                return fail_with_tokens(
                    ctx,
                    node,
                    format!(
                        "node `{}` asked {} question(s) awaiting an answer: {}",
                        node.id,
                        pending.len(),
                        pending.join(", ")
                    ),
                    false,
                    tokens,
                )
                .await;
            }

            ctx.emit(
                Some(&node.id),
                EventPayload::NodeFinished(NodeFinishedPayload {
                    outcome,
                    tokens_used: tokens,
                }),
            )
            .await?;
            write_progress(ctx).await?;
            Ok(NodeEnd::Finished)
        }
        Err(errors) => {
            let listed = errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            fail_with_tokens(ctx, node, listed, false, tokens).await
        }
    }
}

pub(super) async fn fail(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: String,
    retryable: bool,
) -> Result<NodeEnd, RunError> {
    fail_with_tokens(ctx, node, outcome, retryable, TokenUsage::default()).await
}

/// Regenerates `progress.md` at `run.dir`'s root — the engine's own
/// call, right after the `node_finished` that triggers it (`node_failed`
/// is not itself a regeneration point).
pub(super) async fn write_progress(ctx: &RunCtx<'_>) -> Result<(), RunError> {
    let events = ctx.load_events().await?;
    let markdown = crate::progress::render_progress(&ctx.manifest.workflow, &events);
    std::fs::write(ctx.run_dir.join("progress.md"), markdown).map_err(|source| RunError::Io {
        context: "write progress.md".to_string(),
        source,
    })
}

pub(super) async fn fail_with_tokens(
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
    )
    .await?;
    Ok(NodeEnd::Failed)
}
