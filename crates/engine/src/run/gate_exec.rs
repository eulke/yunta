//! `kind: gate` dispatch — the imperative half of what
//! `schedule::ScheduleStep::PublishGate`/`PollGate` name. Reads the
//! declared artifacts off disk, asks the forge to publish/poll, and
//! translates its answer into the same event vocabulary every other
//! node kind already uses (`node_started`/`node_finished`/
//! `node_failed`) — so nothing downstream (`on_failure.goto`,
//! `yunta status`) needs to know a gate is different
//! from any other node once it's resolved. Comments on a
//! changes-requested review become `finding_posted`, mounted for the
//! corrective node the same way `node-output` already is — a reviewer's
//! words reach the fixing session without an intermediate summary.
//!
//! **No forge, no credentials: degrades to console.** Both entry points
//! fall back to [`console_gate`](super::console_gate), which asks here
//! with the same escalation vocabulary every other menu uses. A plan that
//! cannot be proven as it is written is never published:
//! [`flawed`](super::flawed) sends it back instead.

use yunta_core::events::{
    Approval, Claim, EngineFact, Escalation, EventPayload, Fact, Failure, Finding,
    FindingPostedPayload, FindingSeverity, GateResolvedPayload, NodeOutcome, NodeStartedPayload,
    PauseReason, Shown, TokenUsage,
};
use yunta_core::port::{
    Forge, GateDecision, PolledGate, PublishRequest, PullRequestRef, ReviewComment, ReviewOutcome,
};
use yunta_core::{CommitSha, ExternalGate, FindingId, Node, Responder};

use super::node_close::{fail, fail_with, finish_node};
use super::node_exec::template_vars;
use super::pull_request_exec::forge_limit;
use super::step::{GateRender, Step};
use super::{RunCtx, RunError};
use yunta_core::events::{FindingEvent, GateEvent, NodeEvent};
use yunta_core::{Location, RelativePath};

/// What a dispatch call decided — the caller (`run/mod.rs`'s own loop)
/// either keeps going (events already emitted) or pauses and returns.
pub(super) enum GateStep {
    Waiting(PauseReason),
    Resolved,
}

#[tracing::instrument(skip_all, fields(run_id = %ctx.run_id, node_id = %node.id))]
pub(super) async fn publish_gate(
    ctx: &RunCtx<'_>,
    node: &Node,
    assignee: &str,
    message: Option<&str>,
    external: &ExternalGate,
    forge: Option<&dyn Forge>,
) -> Result<GateStep, RunError> {
    let Some(forge) = forge else {
        return super::console_gate::degrade_to_console(
            ctx,
            node,
            Claim::ForgeUnreachable {
                node: node.id.clone(),
                assignee: assignee.to_string(),
            },
        )
        .await;
    };

    let branch = match render_or_fail_here(ctx, node, &external.branch).await? {
        Step::Value(rendered) => rendered,
        Step::Ended(step) => return Ok(step),
    };
    let Some((artifacts, shows)) = held_for_review(ctx, node, external).await? else {
        return Ok(GateStep::Resolved);
    };
    let state = ctx.run_view().await?.state;
    let shown = crate::artifacts::shown::documents(ctx.run_dir, &shows, &state).await?;
    let flaws = super::flawed::unprovable(&shown);
    if !flaws.is_empty() {
        return super::flawed::send_back(ctx, node, &flaws).await;
    }

    let summary = Claim::AwaitsReview {
        node: node.id.clone(),
        assignee: Some(assignee.to_string()),
    };
    let request = PublishRequest {
        branch,
        base_branch: ctx.manifest.base_branch.clone(),
        run_id: ctx.run_id.clone(),
        decision: decision(ctx, node, assignee, message),
        artifacts,
        shown,
    };

    let Some(published) = publish_or_fail(ctx, node, forge, &request).await? else {
        return Ok(GateStep::Resolved);
    };

    emit_waiting(ctx, node, summary, &published).await?;
    Ok(GateStep::Waiting(PauseReason::ExternalGate {
        url: published.url,
    }))
}

/// Publishes the gate's pull request. A forge whose rate limit is spent
/// fails the node with the account's limit, and `None` says the node ended
/// that way; any other refusal is the run's error.
async fn publish_or_fail(
    ctx: &RunCtx<'_>,
    node: &Node,
    forge: &dyn Forge,
    request: &PublishRequest,
) -> Result<Option<PullRequestRef>, RunError> {
    match forge.publish(request).await {
        Ok(published) => Ok(Some(published)),
        Err(error) => {
            let Some(failure) = forge_limit(&error, ctx.clock.now()) else {
                return Err(RunError::Forge {
                    context: format!("publish node `{}`'s external gate", node.id),
                    source: error,
                });
            };
            // Publishing does not start the node, so a refusal starts it
            // first, as every other failure of this step does.
            emit_started(ctx, node).await?;
            fail_with(ctx, node, failure, TokenUsage::default()).await?;
            Ok(None)
        }
    }
}

/// Emits the gate's waiting event, naming the pull request it waits on.
async fn emit_waiting(
    ctx: &RunCtx<'_>,
    node: &Node,
    summary: Claim,
    published: &PullRequestRef,
) -> Result<(), RunError> {
    ctx.emit(
        Some(&node.id),
        EventPayload::Gates(GateEvent::Waiting(
            Escalation::published_to(
                summary,
                vec![Fact::labelled("published at", published.url.clone())].into(),
                encode_ref(published),
            )
            .map_err(|source| RunError::Escalation {
                context: format!("node `{}`'s external gate", node.id),
                source,
            })?
            .into_payload(),
        )),
    )
    .await?;
    Ok(())
}

/// What the reviewer decides on, as the run holds it: each artifact's
/// bytes under the name its identity gives it, and the log's name for
/// each. `None` once the node failed for one the run does not hold — a
/// partial review is never published.
async fn held_for_review(
    ctx: &RunCtx<'_>,
    node: &Node,
    external: &ExternalGate,
) -> Result<Option<(Vec<(String, Vec<u8>)>, Vec<Shown>)>, RunError> {
    let events = ctx.load_events().await?;
    let held = crate::artifacts::RunArtifacts::of(ctx.run_dir, &events);
    let mut artifacts = Vec::new();
    let mut shows = Vec::new();
    for spec in &external.artifacts {
        let wanted = yunta_core::events::ArtifactId::from(spec);
        let Some(artifact) = held.held(&wanted, None) else {
            emit_started(ctx, node).await?;
            let failure = Failure::GateArtifactMissing {
                node: node.id.clone(),
                gate_artifact: wanted,
            };
            fail(ctx, node, failure).await?;
            return Ok(None);
        };
        let bytes = held
            .bytes(artifact)
            .await
            .map_err(crate::artifacts::HeldError::from)?;
        artifacts.push((wanted.view_name(), bytes));
        shows.push(Shown {
            producer: artifact.producer.clone(),
            artifact: artifact.artifact.clone(),
            content_hash: artifact.content_hash.clone(),
        });
    }
    Ok(Some((artifacts, shows)))
}

/// What the gate's pull request asks, and what each answer a review can
/// give does to the run: the nodes that wait on it go on once it passes,
/// and a request for changes goes where its `on_failure` sends it.
fn decision(ctx: &RunCtx<'_>, node: &Node, assignee: &str, message: Option<&str>) -> GateDecision {
    GateDecision {
        node: node.id.clone(),
        question: super::internal_gate::claim(node, message),
        assignee: assignee.to_string(),
        then: ctx
            .manifest
            .workflow
            .nodes
            .iter()
            .filter(|next| next.depends_on.contains(&node.id))
            .map(|next| next.id.clone())
            .collect(),
        corrected_by: node.on_failure.as_ref().map(|on| on.goto.clone()),
    }
}

#[tracing::instrument(skip_all, fields(run_id = %ctx.run_id, node_id = %node.id))]
pub(super) async fn poll_gate(
    ctx: &RunCtx<'_>,
    node: &Node,
    external_ref: &str,
    forge: Option<&dyn Forge>,
) -> Result<GateStep, RunError> {
    let Some(forge) = forge else {
        return super::console_gate::degrade_to_console(
            ctx,
            node,
            Claim::RecheckUnreachable {
                node: node.id.clone(),
                external_ref: external_ref.to_string(),
            },
        )
        .await;
    };

    let published: PullRequestRef = decode_ref(external_ref)?;
    let polled = forge
        .poll(&published)
        .await
        .map_err(|source| RunError::Forge {
            context: format!("poll node `{}`'s external gate", node.id),
            source,
        })?;

    resolve_from_poll(ctx, node, &polled, &published).await
}

/// The review→outcome mapping, applied uniformly whether resolving a
/// gate for the first time or re-checking one already `Finished` for
/// SHA drift (see this module's own doc comment): a review only counts
/// if it covers the PR's *current* head; anything
/// else — no decisive review recorded, or one that no longer covers the
/// current commit — is "still waiting", not a decision.
/// The gate continues: `gate_resolved` records who approved and the
/// commit the approval covers, and the node finishes.
async fn resolve_approved(
    ctx: &RunCtx<'_>,
    node: &Node,
    by: &Responder,
    approved_sha: &CommitSha,
    on: Approval,
) -> Result<GateStep, RunError> {
    emit_started(ctx, node).await?;
    ctx.emit(
        Some(&node.id),
        EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Approved {
            by: by.clone(),
            sha: approved_sha.clone(),
        })),
    )
    .await?;
    finish_node(
        ctx,
        node,
        NodeOutcome::Fact(EngineFact::Approved { by: by.clone(), on }),
        TokenUsage::default(),
    )
    .await?;
    Ok(GateStep::Resolved)
}

async fn resolve_from_poll(
    ctx: &RunCtx<'_>,
    node: &Node,
    polled: &PolledGate,
    published: &PullRequestRef,
) -> Result<GateStep, RunError> {
    match &polled.review {
        ReviewOutcome::Approved { by, reviewed_sha } if *reviewed_sha == polled.head_sha => {
            resolve_approved(ctx, node, by, &polled.head_sha, Approval::Review).await
        }
        // A merge is an approval that also landed: the evidence is the
        // merge commit, and nothing can move the branch after it.
        ReviewOutcome::Merged { by, merge_sha } => {
            resolve_approved(ctx, node, by, merge_sha, Approval::Merge).await
        }
        // Like an approval, a request for changes decides what it
        // reviewed: one left on an earlier head waits for a review of
        // what the correction published.
        ReviewOutcome::ChangesRequested {
            by,
            reviewed_sha,
            comments,
        } if *reviewed_sha == polled.head_sha => {
            resolve_changes_requested(ctx, node, by, reviewed_sha, comments).await
        }
        ReviewOutcome::Closed => resolve_closed(ctx, node, published).await,
        // Pending, or a review that does not cover the current head —
        // the engine detects that by comparing SHAs — not a decision.
        ReviewOutcome::Pending
        | ReviewOutcome::Approved { .. }
        | ReviewOutcome::ChangesRequested { .. } => {
            Ok(GateStep::Waiting(PauseReason::ExternalGate {
                url: published.url.clone(),
            }))
        }
    }
}

/// Resolves a changes-requested review: each comment becomes a finding, and the node fails.
async fn resolve_changes_requested(
    ctx: &RunCtx<'_>,
    node: &Node,
    by: &Responder,
    reviewed_sha: &CommitSha,
    comments: &[ReviewComment],
) -> Result<GateStep, RunError> {
    emit_started(ctx, node).await?;
    ctx.emit(
        Some(&node.id),
        EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::ChangesRequested {
            by: by.clone(),
        })),
    )
    .await?;
    for (i, comment) in comments.iter().enumerate() {
        ctx.emit(
            Some(&node.id),
            EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
                finding: Finding {
                    id: FindingId::try_from(format!("{}-review-{i}", node.id))?,
                    severity: FindingSeverity::Major,
                    title: format!("changes requested by {}", comment.author),
                    location: Location::work(RelativePath::of(comment.path.as_deref()), None),
                    detail: comment.body.clone(),
                    proposed_criterion: None,
                },
            })),
        )
        .await?;
    }
    let failure = Failure::ChangesRequested {
        requested_by: by.to_string(),
        reviewed_sha: reviewed_sha.clone(),
        comments: comments.len(),
    };
    fail(ctx, node, failure).await?;
    Ok(GateStep::Resolved)
}

/// Resolves a gate whose pull request closed without merging: the node fails.
async fn resolve_closed(
    ctx: &RunCtx<'_>,
    node: &Node,
    published: &PullRequestRef,
) -> Result<GateStep, RunError> {
    emit_started(ctx, node).await?;
    ctx.emit(
        Some(&node.id),
        EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Closed)),
    )
    .await?;
    let failure = Failure::PullRequestClosed {
        closed_pull_request: Some(published.url.clone()),
    };
    fail(ctx, node, failure).await?;
    Ok(GateStep::Resolved)
}

/// Re-checks every already-`Finished` `kind: gate` node against its
/// forge's current state — detected by comparing SHAs — run once per
/// `execute_run` invocation, before the scheduling loop, so
/// "on wake" (`resume`, `status`, a scheduled job) means once per
/// wake, not once per scheduling iteration. A node whose approval no
/// longer covers the PR's current head is re-opened (`node_started` at
/// the next attempt) so the ordinary dispatch path re-resolves it fresh
/// — `resolve_from_poll` naturally lands it back on "still waiting"
/// when nothing new has been approved.
///
/// **Only reachable while the run is still open.** Its one caller
/// (`run/mod.rs::execute_run`) places this after the "already
/// `run_finished`" early return — a run the log already closed is
/// immutable, so a stale approval discovered after the whole
/// run finished is a fact for the *next* run to know about, not a
/// reason to reopen a closed one. A workflow whose gate is its very
/// last node therefore never gets re-checked once approved; one with
/// further unresolved work downstream does, on every wake, until that
/// work resolves too.
pub(super) async fn recheck_approved_gates(
    ctx: &RunCtx<'_>,
    forge: Option<&dyn Forge>,
) -> Result<(), RunError> {
    let Some(forge) = forge else {
        return Ok(()); // nothing to re-check without a live forge
    };

    let state = ctx.run_view().await?.state;

    for node in &ctx.manifest.workflow.nodes {
        if !matches!(node.kind, yunta_core::NodeKind::Gate { .. }) {
            continue;
        }
        let Some(crate::replay::NodeState::Finished { .. }) = state.nodes.state(&node.id) else {
            continue;
        };
        let Some(approved_sha) = state.gates.approved_sha(&node.id).cloned() else {
            continue;
        };
        let Some(external_ref) = state.gates.last_external_ref(&node.id) else {
            continue;
        };
        let published: PullRequestRef = decode_ref(external_ref)?;
        let polled = forge
            .poll(&published)
            .await
            .map_err(|source| RunError::Forge {
                context: format!("re-check node `{}`'s external gate", node.id),
                source,
            })?;
        // A merged pull request cannot move, and its evidence is the merge
        // commit rather than the branch head: comparing the two would read
        // every merge as drift and re-open a gate that landed.
        if matches!(polled.review, ReviewOutcome::Merged { .. }) {
            continue;
        }
        if polled.head_sha != approved_sha {
            emit_started(ctx, node).await?;
        }
    }
    Ok(())
}

pub(super) async fn emit_started(ctx: &RunCtx<'_>, node: &Node) -> Result<(), RunError> {
    let state = ctx.run_view().await?.state;
    let attempts = state
        .nodes
        .get(&node.id)
        .map_or(0, |record| record.attempts);
    ctx.emit(
        Some(&node.id),
        EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(
            attempts + 1,
        ))),
    )
    .await?;
    Ok(())
}

fn encode_ref(published: &PullRequestRef) -> String {
    serde_json::to_string(published).unwrap_or_default()
}

fn decode_ref(external_ref: &str) -> Result<PullRequestRef, RunError> {
    serde_json::from_str(external_ref).map_err(|e| RunError::Broken {
        diagnostic: format!("gate_waiting.external_ref `{external_ref}` isn't valid: {e}"),
    })
}

/// Renders `external.branch`'s template, or fails the *run* the same
/// way `node_exec::render_or_fail` fails a node — an undefined
/// `{{...}}` here is a workflow-authoring mistake, not a forge problem.
async fn render_or_fail_here(
    ctx: &RunCtx<'_>,
    node: &Node,
    input: &str,
) -> Result<GateRender, RunError> {
    match yunta_core::template::render_template(input, &template_vars(ctx, node)) {
        Ok(rendered) => Ok(Step::Value(rendered)),
        Err(template) => {
            emit_started(ctx, node).await?;
            fail(ctx, node, Failure::from(template)).await?;
            Ok(Step::Ended(GateStep::Resolved))
        }
    }
}
