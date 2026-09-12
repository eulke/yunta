//! The artifact half of a node's close: the findings file the engine
//! derives from what the node posted, what a `kind: workflow` node takes
//! over from its child run, and the acceptance of everything the close
//! verified.
//!
//! Split from `node_close` because these answer a different question
//! than the lifecycle around them: not how a node ends, but what the run
//! holds once it has.

use std::collections::BTreeMap;

use yunta_core::events::{
    ArtifactId, ArtifactOrigin, EventPayload, TaskStatus, TaskStatusChangedPayload, TokenUsage,
};
use yunta_core::{ArtifactSpec, Node, NodeKind};

use crate::artifacts::{
    accept, canonical, interpreted, ArtifactContent, Declared, RunArtifacts, VerifiedArtifact,
};

use super::node_close::{fail_with_tokens, ChildRun};
use super::node_exec::NodeEnd;
use super::{RunCtx, RunError};

/// The `findings` artifacts `node` declares — the ones whose file is the
/// projection of what that node posted rather than anything it wrote.
fn declared_findings(node: &Node) -> Vec<&ArtifactSpec> {
    node.artifacts
        .iter()
        .flat_map(|artifacts| artifacts.produces.iter())
        .filter(|spec| spec.kind() == Some(yunta_core::ArtifactKind::Findings))
        .collect()
}

/// Writes the findings file of every `findings` artifact a session node
/// declares, from what that node reported.
///
/// Only a node that runs sessions of its own: a `kind: workflow` node's
/// findings are the child run's, acquired from that log by
/// [`acquire_from_child`], and their entries reach this log through
/// `record_artifacts` instead.
pub(super) async fn derive_findings(
    ctx: &RunCtx<'_>,
    node: &Node,
    ceiling: Option<u64>,
    tokens: TokenUsage,
) -> Result<Option<NodeEnd>, RunError> {
    if !matches!(node.kind, NodeKind::Prompt { .. } | NodeKind::Loop { .. }) {
        return Ok(None);
    }
    let declared = declared_findings(node);
    if declared.is_empty() {
        return Ok(None);
    }
    let posted = yunta_core::events::findings::FindingLedger::of(&ctx.load_events().await?)
        .effective_of(&node.id);
    for spec in declared {
        let derived = match crate::artifacts::derive_findings(
            &node.id,
            spec,
            ctx.run_dir,
            posted.clone(),
            ceiling,
        ) {
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

/// Why one artifact a `kind: workflow` node declares did not come across
/// from its child run.
///
/// Each renders as one line of the block the node fails with, under a
/// heading that names the child run — so the run a reader has to go look
/// at is stated once, and each line says which artifact and what about
/// it.
#[derive(Debug, thiserror::Error)]
pub(super) enum NotAcquired {
    #[error(
        "artifact `{name}`: the child run's log holds none — declare it in the child workflow, \
         or drop it from this node's `artifacts.produces`"
    )]
    Unheld { name: String },
    #[error("artifact `{name}`: the child run cannot hand over its bytes: {source}")]
    Unreadable {
        name: String,
        #[source]
        source: crate::artifacts::ObjectError,
    },
    /// The two workflows disagree about what the artifact is: the child
    /// holds one of this identity and it does not read as the kind this
    /// node declares.
    #[error("artifact `{name}`: what the child run holds does not read here: {detail}")]
    Unread { name: String, detail: String },
}

/// One artifact the child's ledger answers for, before the parent
/// accepts it: what this node declares it as, who produced it in the
/// child, the bytes behind it, and what those bytes read as here.
struct Resolved<'a> {
    spec: &'a ArtifactSpec,
    producer: Option<yunta_core::NodeId>,
    bytes: Vec<u8>,
    verified: VerifiedArtifact,
}

/// Every artifact `node` declares, as the child run's ledger answers for
/// it.
///
/// The node's own declaration is the whole question: it says what this
/// composition expects back, so a name it declares becomes an identity
/// (its `kind:`, or the name itself) and that identity is what the
/// child's ledger is asked for. The child may hold more — those are that
/// run's business and stay there — and it must hold these, because a
/// node cannot finish owing what it declared.
///
/// Every problem at once, like any other close: two ends of a
/// composition that disagree about three artifacts say so once, not
/// three times.
fn resolve_from_child<'a>(
    node: &Node,
    produces: &'a [ArtifactSpec],
    held: &RunArtifacts<'_>,
) -> Result<Vec<Resolved<'a>>, Vec<NotAcquired>> {
    let mut resolved = Vec::new();
    let mut problems = Vec::new();
    for spec in produces {
        let name = spec.name().to_string();
        // The declaration in hand is the identity: this node's names are
        // already rendered, so the manifest's own templates cannot
        // answer for them.
        let Some(found) = held
            .ledger()
            .latest(&ArtifactId::of(&name, spec.kind()), None)
        else {
            problems.push(NotAcquired::Unheld { name });
            continue;
        };
        let bytes = match held.bytes(found) {
            Ok(bytes) => bytes,
            Err(source) => {
                problems.push(NotAcquired::Unreadable { name, source });
                continue;
            }
        };
        match interpreted(Some(&node.id), spec, &bytes) {
            Ok(verified) => resolved.push(Resolved {
                spec,
                producer: found.producer.clone(),
                bytes,
                verified,
            }),
            Err(failure) => problems.push(NotAcquired::Unread {
                name,
                detail: yunta_core::text::one_line(&failure.to_string()),
            }),
        }
    }
    if problems.is_empty() {
        Ok(resolved)
    } else {
        Err(problems)
    }
}

/// Takes over every artifact a `kind: workflow` node declares from the
/// log of the child run that produced it.
///
/// Everything resolves before anything is accepted, so a child missing
/// one of them leaves the parent holding none of them. Each acquisition
/// keeps its trail: `Inherited` names the child run and the node that
/// produced the artifact there, and the parent's node is the producer
/// here, so the artifact is this node's own for anything that reads it
/// downstream.
pub(super) async fn acquire_from_child(
    ctx: &RunCtx<'_>,
    node: &Node,
    child: ChildRun<'_>,
) -> Result<Result<Vec<VerifiedArtifact>, Vec<NotAcquired>>, RunError> {
    let Some(artifacts) = &node.artifacts else {
        return Ok(Ok(Vec::new()));
    };
    let events = ctx.storage.events_for_run(child.id.clone()).await?;
    let held = RunArtifacts::of(child.run_dir, &events);
    let resolved = match resolve_from_child(node, &artifacts.produces, &held) {
        Ok(resolved) => resolved,
        Err(problems) => return Ok(Err(problems)),
    };

    let mut acquired = Vec::with_capacity(resolved.len());
    for item in resolved {
        accept(
            &ctx.log(),
            ctx.run_dir,
            Some(&node.id),
            Declared {
                name: item.spec.name(),
                kind: item.spec.kind(),
            },
            &item.bytes,
            ArtifactOrigin::Inherited {
                run: child.id.clone(),
                producer: item.producer,
            },
        )
        .await?;
        acquired.push(item.verified);
    }
    Ok(Ok(acquired))
}

/// Whether the run already holds this artifact, under the origin that
/// brought it in — which is what says the close has nothing to accept.
///
/// A typed artifact of a session node is never a file that session
/// wrote: `tasks` and `questions` arrive through the submission tool and
/// `findings` are derived from what the node posted. A `kind: workflow`
/// node produces no file at all: everything it declares is taken over
/// from its child run's log. Each is accepted where it is produced, with
/// the origin that produced it. Everything else the close verifies is a
/// file the node wrote, and the close is where it enters the run.
fn already_held(node: &Node, artifact: &VerifiedArtifact) -> bool {
    match node.kind {
        NodeKind::Prompt { .. } | NodeKind::Loop { .. } => artifact.content.kind().is_some(),
        NodeKind::Workflow { .. } => true,
        _ => false,
    }
}

/// Records every verified artifact on the log, and what its content
/// means to the run: a tasks document's tasks registered, a findings
/// file's entries posted.
///
/// Only what the run does not already hold enters here, as `ingested` —
/// everything else is on the log already, with the origin that brought
/// it in. What the run stores is the canonical rendering, so an
/// interpreted document is the same bytes whoever wrote the file.
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
        if !already_held(node, artifact) {
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
            // node's were posted in the child run, so this log learns
            // them here, under the node that acquired them: the parent
            // counts, renders and inherits the composition's findings
            // like any other.
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
