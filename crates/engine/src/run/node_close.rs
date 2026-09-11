//! How a node ends: verifying what it declared, recording a failure, and
//! refreshing `progress.md` either way.
//!
//! Every kind closes through [`close_node`], so every kind gets the same
//! after-hooks, the same scope check, the same artifact verification and
//! the same repair cycle — none of them can forget one. Every node
//! failure the engine records goes through the `fail*` family here, so
//! `node_failed` is written in one place and a new field on the event is
//! a change to one function rather than to every kind of node.

use std::collections::BTreeMap;
use std::path::PathBuf;

use tokio_util::sync::CancellationToken;
use yunta_core::diagnostic::ArtifactFailure;
use yunta_core::events::{
    EventPayload, Failure, HookPhase, NodeFailedPayload, NodeFinishedPayload, TaskStatus,
    TaskStatusChangedPayload, TokenUsage,
};
use yunta_core::{HookFailurePolicy, Node};

use crate::artifacts::{close_artifacts, ArtifactContent, VerifiedArtifact};
use crate::scope::scope_check;

use super::hooks_exec::{effective_hooks, run_hook, HookRun};
use super::node_exec::{render_artifact_names, NodeEnd};
use super::repair;
use super::{RunCtx, RunError};

/// What a node's close needs beyond the node itself.
///
/// Grouped rather than passed loose because every field answers a
/// question only the caller can: how the node ended, what it spent,
/// which attempt it is closing (a repair announces the next one), what
/// the adapter declared it wrote for itself, and which cancellation a
/// repair session runs under.
pub(super) struct Close<'a> {
    outcome: String,
    tokens: TokenUsage,
    /// The attempt being closed. A repair announces the next one.
    pub(super) attempt: u32,
    /// The cancellation a repair session runs under.
    pub(super) cancel: &'a CancellationToken,
    staged: &'a [PathBuf],
}

impl<'a> Close<'a> {
    /// A close that staged nothing in the worktree — every kind but an
    /// agent session, whose adapter writes files of its own that the
    /// scope diff must leave out.
    pub(super) fn new(
        outcome: impl Into<String>,
        tokens: TokenUsage,
        attempt: u32,
        cancel: &'a CancellationToken,
    ) -> Self {
        Close {
            outcome: outcome.into(),
            tokens,
            attempt,
            cancel,
            staged: &[],
        }
    }

    /// The paths the adapter declared it wrote for itself, left out of
    /// the scope diff.
    pub(super) fn staged(mut self, staged: &'a [PathBuf]) -> Self {
        self.staged = staged;
        self
    }
}

/// Closes a node: its `after` hooks run, its scope is checked over the
/// whole diff — hook edits included, staged paths left out — its
/// declared artifacts are verified (repaired, if their content is wrong
/// and it resolves a runner to fix them), and it finishes.
pub(super) async fn close_node(
    ctx: &RunCtx<'_>,
    node: &Node,
    close: Close<'_>,
) -> Result<NodeEnd, RunError> {
    let tokens = close.tokens;
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

    if let Some(end) = scope_violation(ctx, node, close.staged, tokens).await? {
        return Ok(end);
    }

    // Artifact names are templates too (`findings-{{runner.role}}`
    // in the reference workflow) — rendered per node so every fan-out
    // sibling verifies its own file.
    let node_rendered = match render_artifact_names(ctx, node) {
        Ok(rendered) => rendered,
        Err(error) => return fail_with_tokens(ctx, node, error.to_string(), false, tokens).await,
    };
    let node = &node_rendered;

    let ceiling = ctx
        .manifest
        .config
        .limits
        .as_ref()
        .and_then(|limits| limits.max_artifact_bytes);
    let mut tokens = tokens;
    let mut spent = 0;
    let verified = loop {
        match close_artifacts(node, ctx.run_dir, ceiling) {
            Ok(verified) => break verified,
            // Every `node_failed` on the way is recorded by the cycle,
            // including the one that ends it.
            Err(failures) => {
                match repair::next(ctx, node, &close, spent, tokens, failures).await? {
                    repair::Next::Ended(end) => return Ok(end),
                    repair::Next::Wrote(spend) => {
                        tokens = spend;
                        spent += 1;
                        // A repair session is a session like any other, so
                        // what it wrote sits inside the same scope guard as
                        // what the node wrote: the diff is checked again
                        // before its files are read.
                        if let Some(end) = scope_violation(ctx, node, close.staged, tokens).await? {
                            return Ok(end);
                        }
                    }
                }
            }
        }
    };

    record_artifacts(ctx, node, &verified).await?;
    let pending = pending_questions(&verified);
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
            outcome: close.outcome,
            tokens_used: tokens,
        }),
    )
    .await?;
    write_progress(ctx).await?;
    Ok(NodeEnd::Finished)
}

/// The node's whole diff against its declared `scope:`, audited as
/// `scope_checked` and failing the node when anything falls outside.
/// `staged` is what the adapter declared it wrote for itself, which is
/// not the node's doing and so is not the node's diff.
///
/// `None` when the node declares no scope, or when its diff is inside
/// it — a node that declares nothing constrains nothing.
async fn scope_violation(
    ctx: &RunCtx<'_>,
    node: &Node,
    staged: &[PathBuf],
    tokens: TokenUsage,
) -> Result<Option<NodeEnd>, RunError> {
    if node.scope.is_empty() {
        return Ok(None);
    }
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
    if result.violations.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        fail_with_tokens(
            ctx,
            node,
            format!(
                "scope violated: {} file(s) outside the declared globs",
                result.violations.len()
            ),
            false,
            tokens,
        )
        .await?,
    ))
}

/// Records every verified artifact on the log, and what its content
/// means to the run: a ledger's tasks registered, a findings file's
/// entries posted.
///
/// A re-plan — this same node producing a task ledger a second time,
/// whether via a reroute back to it or a resumed run — must not silently
/// keep a task `done` whose identity actually changed. Identity is same
/// `id`, same `criteria`, same `scope`; `depends_on` is deliberately not
/// part of it. The most recent prior registration per task id is all
/// that is needed, because `TaskRegistered`'s own replay handling
/// (`or_insert`, never overwriting an existing status) already makes an
/// identical re-registration a no-op — so only a genuine mismatch needs
/// an explicit event.
async fn record_artifacts(
    ctx: &RunCtx<'_>,
    node: &Node,
    verified: &[VerifiedArtifact],
) -> Result<(), RunError> {
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

    for artifact in verified {
        ctx.emit(
            Some(&node.id),
            EventPayload::ArtifactWritten(yunta_core::events::ArtifactWrittenPayload {
                path: artifact.path.clone(),
                content_hash: artifact.content_hash.clone(),
                artifact_kind: artifact.content.kind(),
            }),
        )
        .await?;
        match &artifact.content {
            ArtifactContent::TaskLedger(ledger) => {
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
                    let changed_identity =
                        previous_registrations
                            .get(&task.id)
                            .is_some_and(|(previous, scope)| {
                                *previous != criteria || *scope != task.scope
                            });
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
            ArtifactContent::Findings(findings) => {
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
            ArtifactContent::Questions(_) | ArtifactContent::Opaque => {}
        }
    }
    Ok(())
}

/// Every question this node's artifacts ask, by id.
///
/// A `kind: questions` artifact's own session has already closed by the
/// time it is read (the same "artifact read only at node close" ordering
/// `task-ledger` and `findings` rely on), so nothing renders
/// mid-session. Questions left here close the node waiting-shaped — a
/// `node_failed` that replay derives as `Waiting` from the typed
/// `kind: questions` on the artifact event — and the asking happens in
/// ONE place, the scheduler's own `AskQuestions` step
/// (`questions_exec`), which serves the first invocation and every
/// resume through the identical path.
fn pending_questions(verified: &[VerifiedArtifact]) -> Vec<String> {
    verified
        .iter()
        .filter_map(|artifact| match &artifact.content {
            ArtifactContent::Questions(questions) => Some(questions),
            _ => None,
        })
        .flatten()
        .map(|question| question.id.to_string())
        .collect()
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

/// Fails a node with a failure the engine states in one sentence — a
/// hook's exit code, a budget, a runner that does not resolve. Such a
/// failure is not missing its diagnostics: it simply is not about a
/// document.
pub(super) async fn fail_with_tokens(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: String,
    retryable: bool,
    tokens: TokenUsage,
) -> Result<NodeEnd, RunError> {
    record(ctx, node, Failure::message(outcome), retryable, tokens).await
}

/// Fails a node with the artifacts that did not close, each keeping the
/// document its problems belong to. The prose a reader sees is produced
/// from this on read, so no surface can disagree with the facts behind
/// it.
pub(super) async fn fail_artifacts(
    ctx: &RunCtx<'_>,
    node: &Node,
    failures: Vec<ArtifactFailure>,
    retryable: bool,
    tokens: TokenUsage,
) -> Result<NodeEnd, RunError> {
    record(ctx, node, Failure::artifacts(failures), retryable, tokens).await
}

/// The one place `node_failed` is written.
async fn record(
    ctx: &RunCtx<'_>,
    node: &Node,
    failure: Failure,
    retryable: bool,
    tokens: TokenUsage,
) -> Result<NodeEnd, RunError> {
    ctx.emit(
        Some(&node.id),
        EventPayload::NodeFailed(NodeFailedPayload::new(failure, retryable, tokens)),
    )
    .await?;
    Ok(NodeEnd::Failed)
}
