//! How a node ends: verifying what it declared, recording a failure, and
//! refreshing `progress.md` either way.
//!
//! Every kind closes through [`close_node`], so every kind gets the same
//! after-hooks, the same scope check and the same artifact verification
//! — none of them can forget one. Every node failure the engine records
//! goes through the `fail*` family here, so `node_failed` is written in
//! one place and a new field on the event is a change to one function
//! rather than to every kind of node.

use std::collections::BTreeMap;
use std::path::PathBuf;

use yunta_core::diagnostic::ArtifactFailure;
use yunta_core::events::{
    EventPayload, Failure, HookPhase, NodeFailedPayload, NodeFinishedPayload, TaskStatus,
    TaskStatusChangedPayload, TokenUsage,
};
use yunta_core::{ArtifactSpec, NodeKind};
use yunta_core::{HookFailurePolicy, Node};

use crate::artifacts::{close_artifacts, ArtifactContent, ArtifactsSnapshot, VerifiedArtifact};
use crate::scope::scope_check;

use super::hooks_exec::{effective_hooks, run_hook, HookRun};
use super::node_exec::{render_artifact_names, NodeEnd};
use super::{RunCtx, RunError};

/// What a node's close needs beyond the node itself.
///
/// Grouped rather than passed loose because every field answers a
/// question only the caller can: how the node ended, what it spent, and
/// what the adapter declared it wrote for itself.
pub(super) struct Close<'a> {
    outcome: String,
    tokens: TokenUsage,
    staged: &'a [PathBuf],
    artifacts_before: Option<&'a ArtifactsSnapshot>,
}

impl<'a> Close<'a> {
    /// A close that staged nothing in the worktree — every kind but an
    /// agent session, whose adapter writes files of its own that the
    /// scope diff must leave out.
    pub(super) fn new(outcome: impl Into<String>, tokens: TokenUsage) -> Self {
        Close {
            outcome: outcome.into(),
            tokens,
            staged: &[],
            artifacts_before: None,
        }
    }

    /// The paths the adapter declared it wrote for itself, left out of
    /// the scope diff.
    pub(super) fn staged(mut self, staged: &'a [PathBuf]) -> Self {
        self.staged = staged;
        self
    }

    /// What the run's `artifacts/` held before this node's session ran,
    /// which is what makes the files it wrote there tellable from the
    /// files every other node of the run already owns.
    pub(super) fn artifacts_before(mut self, before: &'a ArtifactsSnapshot) -> Self {
        self.artifacts_before = Some(before);
        self
    }
}

/// Closes a node: its `after` hooks run, its scope is checked over the
/// whole diff — hook edits included, staged paths left out — its
/// declared artifacts are verified, and it finishes.
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

    if let Some(end) = artifacts_violation(ctx, node, &close, tokens).await? {
        return Ok(end);
    }

    let ceiling = ctx
        .manifest
        .config
        .limits
        .as_ref()
        .and_then(|limits| limits.max_artifact_bytes);
    // A session node's findings artifact is what that node reported, so
    // the engine writes it here — before the one read the close does, and
    // from the log rather than from anything a session left on disk.
    if let Some(end) = derive_findings(ctx, node, ceiling, tokens).await? {
        return Ok(end);
    }
    let verified = match close_artifacts(node, ctx.run_dir, ceiling) {
        Ok(verified) => verified,
        Err(failures) => return fail_artifacts(ctx, node, failures, false, tokens).await,
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
/// `None` when the node owes no audit at all, or when its diff is
/// inside what it may touch. Which scope that is — a declared one, or
/// nothing whatsoever for a `read-only` node — is
/// [`audited_scope`](crate::audited_scope)'s call, not this one's.
async fn scope_violation(
    ctx: &RunCtx<'_>,
    node: &Node,
    staged: &[PathBuf],
    tokens: TokenUsage,
) -> Result<Option<NodeEnd>, RunError> {
    let Some(scope) = crate::audited_scope(node) else {
        return Ok(None);
    };
    let result = scope_check(ctx.worktree, scope, staged).await?;
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

/// What the node's session wrote into the run's shared `artifacts/`,
/// audited against the artifacts the node declares it produces.
///
/// The worktree and that directory are the two surfaces a session can
/// write and neither is confined by the CLI: a scope is a promise, and
/// `artifacts/` is one directory every node of the run shares. So both
/// are audited the same way — the session writes, and the close holds
/// what it wrote against what it was allowed to write.
///
/// `None` when the node owes no audit — a close with no session behind
/// it — or when everything that changed is the node's own to write.
async fn artifacts_violation(
    ctx: &RunCtx<'_>,
    node: &Node,
    close: &Close<'_>,
    tokens: TokenUsage,
) -> Result<Option<NodeEnd>, RunError> {
    let Some(before) = close.artifacts_before else {
        return Ok(None);
    };
    let declared: Vec<String> = node
        .artifacts
        .iter()
        .flat_map(|artifacts| artifacts.produces.iter())
        .map(|spec| spec.name().to_string())
        .collect();
    let undeclared = before
        .undeclared_writes(ctx.run_dir, &declared)
        .map_err(|source| RunError::Io {
            context: format!("audit what node `{}` wrote under `artifacts/`", node.id),
            source,
        })?;
    if undeclared.is_empty() {
        return Ok(None);
    }
    let names = undeclared
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Ok(Some(
        fail_with_tokens(
            ctx,
            node,
            format!(
                "wrote {} file(s) under `artifacts/` this node never declared: {names} — \
                 declare them under `artifacts.produces`, or leave them to the node that does",
                undeclared.len()
            ),
            false,
            tokens,
        )
        .await?,
    ))
}

/// Records every verified artifact on the log, and what its content
/// means to the run: a tasks document's tasks registered, a findings file's
/// entries posted.
///
/// A re-plan — this same node producing a tasks document a second time,
/// whether via a reroute back to it or a resumed run — must not silently
/// keep a task `done` whose identity actually changed. Identity is same
/// `id`, same `criteria`, same `scope`; `depends_on` is deliberately not
/// part of it. The most recent prior registration per task id is all
/// that is needed, because `TaskRegistered`'s own replay handling
/// (`or_insert`, never overwriting an existing status) already makes an
/// identical re-registration a no-op — so only a genuine mismatch needs
/// an explicit event.
/// Writes the findings file of every `findings` artifact a session node
/// declares, from what that node reported.
///
/// Only a node that runs sessions of its own: a `kind: workflow` node's
/// findings come from its child run, already a file, and its entries
/// reach this log through `record_artifacts` instead.
async fn derive_findings(
    ctx: &RunCtx<'_>,
    node: &Node,
    ceiling: Option<u64>,
    tokens: TokenUsage,
) -> Result<Option<NodeEnd>, RunError> {
    if !matches!(node.kind, NodeKind::Prompt { .. } | NodeKind::Loop { .. }) {
        return Ok(None);
    }
    let declared: Vec<&ArtifactSpec> = node
        .artifacts
        .iter()
        .flat_map(|artifacts| artifacts.produces.iter())
        .filter(|spec| {
            matches!(
                spec,
                ArtifactSpec::Typed {
                    kind: yunta_core::ArtifactKind::Findings,
                    ..
                }
            )
        })
        .collect();
    if declared.is_empty() {
        return Ok(None);
    }
    let posted = yunta_core::events::findings::FindingLedger::of(&ctx.load_events().await?)
        .effective_of(&node.id);
    for spec in declared {
        if let Err(error) =
            crate::artifacts::derive_findings(spec, ctx.run_dir, posted.clone(), ceiling)
        {
            return Ok(Some(
                fail_with_tokens(ctx, node, error.to_string(), false, tokens).await?,
            ));
        }
    }
    Ok(None)
}

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
            ArtifactContent::Tasks(tasks) => {
                for task in &tasks.tasks {
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
            // A session node's findings are already on this log — they
            // are what the file was derived from. A `kind: workflow`
            // node's come from the child run's log, so this one learns
            // them here.
            ArtifactContent::Findings(findings)
                if matches!(node.kind, NodeKind::Workflow { .. }) =>
            {
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
            ArtifactContent::Findings(_)
            | ArtifactContent::Questions(_)
            | ArtifactContent::Opaque => {}
        }
    }
    Ok(())
}

/// Every question this node's artifacts ask, by id.
///
/// A `kind: questions` artifact's own session has already closed by the
/// time it is read (the same "artifact read only at node close" ordering
/// `tasks` and `findings` rely on), so nothing renders
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
