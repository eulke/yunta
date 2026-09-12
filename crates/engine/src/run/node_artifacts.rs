//! The artifact half of a node's close: what its session wrote into the
//! run's shared `artifacts/`, the findings file the engine derives from
//! what the node posted, and the acceptance of everything the close
//! verified.
//!
//! Split from `node_close` because these answer a different question
//! than the lifecycle around them: not how a node ends, but what the run
//! holds once it has.

use std::collections::BTreeMap;

use yunta_core::events::{
    ArtifactOrigin, EventPayload, TaskStatus, TaskStatusChangedPayload, TokenUsage,
};
use yunta_core::{ArtifactSpec, Node, NodeKind};

use crate::artifacts::{
    accept, canonical, ArtifactContent, ArtifactsSnapshot, Declared, VerifiedArtifact,
};

use super::node_close::fail_with_tokens;
use super::node_exec::NodeEnd;
use super::{RunCtx, RunError};

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
pub(super) async fn artifacts_violation(
    ctx: &RunCtx<'_>,
    node: &Node,
    before: Option<&ArtifactsSnapshot>,
    tokens: TokenUsage,
) -> Result<Option<NodeEnd>, RunError> {
    let Some(before) = before else {
        return Ok(None);
    };
    let declared: Vec<String> = node
        .artifacts
        .iter()
        .flat_map(|artifacts| artifacts.produces.iter())
        .map(|spec| spec.name().to_string())
        .collect();
    let producers: Vec<&yunta_core::NodeId> =
        ctx.manifest.workflow.iter_nodes().map(|n| &n.id).collect();
    let undeclared = before
        .undeclared_writes(ctx.run_dir, &declared, &producers)
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

/// Writes the findings file of every `findings` artifact a session node
/// declares, from what that node reported.
///
/// Only a node that runs sessions of its own: a `kind: workflow` node's
/// findings come from its child run, already a file, and its entries
/// reach this log through `record_artifacts` instead.
pub(super) async fn derive_findings(
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
        let derived =
            match crate::artifacts::derive_findings(spec, ctx.run_dir, posted.clone(), ceiling) {
                Ok(derived) => derived,
                Err(error) => {
                    return Ok(Some(
                        fail_with_tokens(ctx, node, error.to_string(), false, tokens).await?,
                    ))
                }
            };
        accept(
            &ctx.log(),
            ctx.run_dir,
            Some(&node.id),
            Declared {
                name: &derived.name,
                kind: Some(yunta_core::ArtifactKind::Findings),
            },
            &derived.bytes,
            ArtifactOrigin::Derived,
        )
        .await?;
    }
    Ok(None)
}

/// Whether the engine itself produced this artifact's bytes, which is
/// what says the run already holds it.
///
/// A typed artifact of a session node is never a file that session
/// wrote: `tasks` and `questions` arrive through the submission tool and
/// `findings` are derived from what the node posted, and each is
/// accepted where it is produced. Everything else the close verifies is
/// a file the node wrote, and the close is where it enters the run.
fn engine_written(node: &Node, artifact: &VerifiedArtifact) -> bool {
    matches!(node.kind, NodeKind::Prompt { .. } | NodeKind::Loop { .. })
        && artifact.content.kind().is_some()
}

/// Records every verified artifact on the log, and what its content
/// means to the run: a tasks document's tasks registered, a findings
/// file's entries posted.
///
/// Only what the engine did not produce itself enters here, as
/// `ingested` — everything else is already on the log with the origin
/// that produced it. What the run stores is the canonical rendering, so
/// an interpreted document is the same bytes whoever wrote the file.
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
pub(super) async fn record_artifacts(
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
        if !engine_written(node, artifact) {
            let bytes = canonical(artifact).map_err(|error| RunError::Broken {
                diagnostic: error.to_string(),
            })?;
            accept(
                &ctx.log(),
                ctx.run_dir,
                Some(&node.id),
                Declared {
                    name: &artifact.name,
                    kind: artifact.content.kind(),
                },
                &bytes,
                ArtifactOrigin::Ingested,
            )
            .await?;
        }
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
pub(super) fn pending_questions(verified: &[VerifiedArtifact]) -> Vec<String> {
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
