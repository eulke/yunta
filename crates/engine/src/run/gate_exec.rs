//! `kind: gate` dispatch — the imperative half of what
//! `schedule::ScheduleStep::PublishGate`/`PollGate` name. Reads the
//! declared artifacts off disk, asks the forge to publish/poll, and
//! translates its answer into the same event vocabulary every other
//! node kind already uses (`node_started`/`node_finished`/
//! `node_failed`) — so nothing downstream (`on_failure.goto`,
//! `progress.md`, `yunta status`) needs to know a gate is different
//! from any other node once it's resolved. Comments on a
//! changes-requested review become `finding_posted`, mounted for the
//! corrective node the same way `node-output` already is — a reviewer's
//! words reach the fixing session without an intermediate summary.
//!
//! **No forge, no credentials: degrades to console.** Both entry points
//! fall back to the exact same `HumanInteraction` escalation
//! `GateExhaustedReroutes` already uses — same object, same "`None`
//! means pause, never guess" rule — building a synthetic PR-less
//! decision instead of a forge round-trip. Nothing is ever recorded as
//! published while degraded (no `gate_waiting` without a resolved
//! answer alongside it, mirroring `GateExhaustedReroutes`'s own pattern
//! exactly), so a still-unresolved degraded gate asks fresh on every
//! wake rather than remembering a decision that was never really made.

use yunta_core::events::{
    Escalation, EventPayload, Fact, Finding, FindingPostedPayload, FindingSeverity,
    GateResolvedPayload, NodeFinishedPayload, NodeStartedPayload, PauseReason, TokenUsage,
};
use yunta_core::port::{Forge, PolledGate, PublishRequest, PublishedGate, ReviewOutcome};
use yunta_core::{CommitSha, ExternalGate, FindingId, Node, NonEmpty, OptionId, Responder};

use super::node_close::{fail, write_progress};
use super::node_exec::template_vars;
use super::step::{GateRender, Step};
use super::{RunCtx, RunError};
use crate::reserved::{offers, ReservedOption};
use yunta_core::events::{FindingEvent, GateEvent, NodeEvent};

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
    external: &ExternalGate,
    forge: Option<&dyn Forge>,
) -> Result<GateStep, RunError> {
    let Some(forge) = forge else {
        return degrade_to_console(
            ctx,
            node,
            format!(
                "node `{}` wants an external gate (assignee: {assignee}) but no forge is \
                 reachable from this machine (no credentials, or none configured) — resolve \
                 it here instead",
                node.id
            ),
        )
        .await;
    };

    let branch = match render_or_fail_here(ctx, node, &external.branch).await? {
        Step::Value(rendered) => rendered,
        Step::Ended(step) => return Ok(step),
    };

    // What the reviewer is shown is what the run holds: the acceptance
    // its log states, with the bytes out of its object store.
    let events = ctx.load_events().await?;
    let held = crate::artifacts::RunArtifacts::of(ctx.run_dir, &events);
    let mut artifacts = Vec::new();
    for spec in &external.artifacts {
        let wanted = yunta_core::events::ArtifactId::from(spec);
        let Some(artifact) = held.held(&wanted, None) else {
            emit_started(ctx, node).await?;
            fail(
                ctx,
                node,
                format!(
                    "node `{}` publishes the {} with its external gate, and this run holds \
                     no such artifact — no node produced it",
                    node.id,
                    wanted.label()
                ),
                false,
            )
            .await?;
            return Ok(GateStep::Resolved);
        };
        artifacts.push((wanted.view_name(), held.bytes(artifact).await?));
    }

    let summary = format!(
        "node `{}` is waiting on external review (assignee: {assignee})",
        node.id
    );
    let request = PublishRequest {
        branch,
        base_branch: ctx.manifest.base_branch.clone(),
        run_id: ctx.run_id.to_string(),
        summary: summary.clone(),
        artifacts,
    };

    let published = forge
        .publish(&request)
        .await
        .map_err(|source| RunError::Forge {
            context: format!("publish node `{}`'s external gate", node.id),
            source,
        })?;

    ctx.emit(
        Some(&node.id),
        EventPayload::Gates(GateEvent::Waiting(
            Escalation::published_to(
                summary,
                vec![Fact::labelled("published at", published.url.clone())].into(),
                encode_ref(&published),
            )
            .map_err(|source| RunError::Broken {
                diagnostic: format!("node `{}`'s external gate: {source}", node.id),
            })?
            .into_payload(),
        )),
    )
    .await?;
    Ok(GateStep::Waiting(PauseReason::ExternalGate {
        url: published.url,
    }))
}

#[tracing::instrument(skip_all, fields(run_id = %ctx.run_id, node_id = %node.id))]
pub(super) async fn poll_gate(
    ctx: &RunCtx<'_>,
    node: &Node,
    external_ref: &str,
    forge: Option<&dyn Forge>,
) -> Result<GateStep, RunError> {
    let Some(forge) = forge else {
        return degrade_to_console(
            ctx,
            node,
            format!(
                "node `{}`'s external gate (published at {external_ref}) needs re-checking, \
                 but no forge is reachable from this machine — resolve it here instead",
                node.id
            ),
        )
        .await;
    };

    let published: PublishedGate = decode_ref(external_ref)?;
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
/// else — no decisive review yet, or one that no longer covers the
/// current commit — is "still waiting", not a decision.
/// The gate continues: `gate_resolved` records who approved and the
/// commit the approval covers, and the node finishes.
async fn resolve_approved(
    ctx: &RunCtx<'_>,
    node: &Node,
    by: &Responder,
    approved_sha: &CommitSha,
    outcome: String,
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
    ctx.emit(
        Some(&node.id),
        EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload {
            outcome,
            tokens_used: TokenUsage::default(),
        })),
    )
    .await?;
    write_progress(ctx).await?;
    Ok(GateStep::Resolved)
}

async fn resolve_from_poll(
    ctx: &RunCtx<'_>,
    node: &Node,
    polled: &PolledGate,
    published: &PublishedGate,
) -> Result<GateStep, RunError> {
    match &polled.review {
        ReviewOutcome::Approved { by, reviewed_sha } if *reviewed_sha == polled.head_sha => {
            resolve_approved(ctx, node, by, &polled.head_sha, format!("approved by {by}")).await
        }
        // A merge is an approval that also landed: the evidence is the
        // merge commit, and nothing can move the branch after it.
        ReviewOutcome::Merged { by, merge_sha } => {
            resolve_approved(ctx, node, by, merge_sha, format!("merged by {by}")).await
        }
        ReviewOutcome::ChangesRequested {
            by,
            reviewed_sha,
            comments,
        } => {
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
                            location: comment
                                .path
                                .clone()
                                .unwrap_or_else(|| "(pull request)".to_string()),
                            detail: comment.body.clone(),
                            proposed_criterion: None,
                        },
                    })),
                )
                .await?;
            }
            fail(
                ctx,
                node,
                format!(
                    "changes requested by {by} at {reviewed_sha} ({} comment(s))",
                    comments.len()
                ),
                true,
            )
            .await?;
            Ok(GateStep::Resolved)
        }
        ReviewOutcome::Closed => {
            emit_started(ctx, node).await?;
            ctx.emit(
                Some(&node.id),
                EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Closed)),
            )
            .await?;
            fail(
                ctx,
                node,
                "the pull request was closed without approval".to_string(),
                false,
            )
            .await?;
            Ok(GateStep::Resolved)
        }
        // Pending, or an approval that no longer covers the current
        // head — the engine detects that by comparing SHAs — not a
        // decision.
        ReviewOutcome::Pending | ReviewOutcome::Approved { .. } => {
            Ok(GateStep::Waiting(PauseReason::ExternalGate {
                url: published.url.clone(),
            }))
        }
    }
}

/// Re-checks every already-`Finished` `kind: gate` node against its
/// forge's current state — detected by comparing SHAs — run once per
/// `execute_run` invocation, before the scheduling loop, so
/// "al despertar" (`resume`, `status`, a scheduled job) means once per
/// wake, not once per scheduling iteration. A node whose approval no
/// longer covers the PR's current head is re-opened (`node_started` at
/// the next attempt) so the ordinary dispatch path re-resolves it fresh
/// — `resolve_from_poll` naturally lands it back on "still waiting"
/// when nothing new has been approved yet.
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

    let crate::replay::RunView { events, state } = ctx.run_view().await?;

    for node in &ctx.manifest.workflow.nodes {
        if !matches!(node.kind, yunta_core::NodeKind::Gate { .. }) {
            continue;
        }
        let Some(crate::replay::NodeState::Finished { .. }) = state.nodes.state(&node.id) else {
            continue;
        };
        let state = crate::replay::derive(&events);
        let Some(approved_sha) = state.gates.approved_sha(&node.id).cloned() else {
            continue;
        };
        let Some(external_ref) = state.gates.last_external_ref(&node.id) else {
            continue;
        };
        let published: PublishedGate = decode_ref(external_ref)?;
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

/// Resolves an internal gate (`external: None`): builds the escalation
/// object from the node's own `message`/`options`/`on` and puts it to
/// `HumanInteraction`. Semantics: an option mapped in `on` re-routes
/// exactly like `on_failure.goto` — the gate fails retryable, control
/// transfers, and once the target's subgraph completes the gate asks
/// again; an unmapped option finishes the gate with that choice as its
/// outcome; the engine-appended `abort` pauses the run (same convention
/// as every other escalation). No surface → `Waiting`, with
/// nothing recorded, so a resume re-asks (the same rule every
/// unresolved question follows).
#[tracing::instrument(skip_all, fields(run_id = %ctx.run_id, node_id = %node.id))]
pub(super) async fn resolve_internal_gate(
    ctx: &RunCtx<'_>,
    node: &Node,
    assignee: &str,
    message: Option<&str>,
    options: &[OptionId],
    on: &indexmap::IndexMap<OptionId, yunta_core::NodeId>,
) -> Result<GateStep, RunError> {
    // Shared with `current_escalation` so a `resolve_gate`
    // MCP call, running in a process that never paused this run,
    // reconstructs the identical object instead of a second copy that
    // could drift.
    let escalation =
        super::escalation::build_internal_gate_escalation(&node.id, assignee, message, options, on)
            .map_err(|source| RunError::Broken {
                diagnostic: format!("node `{}`'s gate: {source}", node.id),
            })?;
    // A decision `resolve_gate` pre-seeded onto the log while
    // this run was parked is consumed here, by this same consequence
    // code — never re-asked, and its escalation pair is already
    // recorded so it is never re-emitted. Re-validated against the
    // re-derived menu: a mismatch means ask normally.
    let events = ctx.load_events().await?;
    let pre_seeded = super::escalation::pre_seeded_resolution(
        &crate::replay::derive(&events),
        &node.id,
        &escalation,
    );
    let already_recorded = pre_seeded.is_some();
    let choice = match pre_seeded {
        Some(choice) => choice,
        None => match ctx.ask_human(&escalation).await? {
            Some(choice) => choice,
            None => {
                return Ok(GateStep::Waiting(PauseReason::Escalation(Box::new(
                    escalation.clone(),
                ))));
            }
        },
    };

    // Whether `abort` is the engine's own appended option (never the
    // author's) — the same rule `build_internal_gate_escalation`
    // applies when it decides whether to append it at all.
    let engine_abort = !options
        .iter()
        .any(|id| ReservedOption::of(id) == Some(ReservedOption::Abort));
    if engine_abort && ReservedOption::of(&choice.option) == Some(ReservedOption::Abort) {
        // The usual escalation convention exactly: record the
        // interaction, pause the run, leave the node stateless so a
        // resume re-asks if the human changes their mind.
        if !already_recorded {
            ctx.emit(
                Some(&node.id),
                EventPayload::Gates(GateEvent::Waiting(escalation.clone().into_payload())),
            )
            .await?;
            ctx.emit(
                Some(&node.id),
                EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Chosen(
                    choice.clone(),
                ))),
            )
            .await?;
        }
        return Ok(GateStep::Waiting(PauseReason::GateAborted {
            node: node.id.clone(),
            free_text: choice.free_text.clone(),
        }));
    }

    emit_started(ctx, node).await?;
    if !already_recorded {
        ctx.emit(
            Some(&node.id),
            EventPayload::Gates(GateEvent::Waiting(escalation.clone().into_payload())),
        )
        .await?;
        ctx.emit(
            Some(&node.id),
            EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Chosen(
                choice.clone(),
            ))),
        )
        .await?;
    }
    let chosen = choice.option;
    match on.get(&chosen) {
        Some(target) => {
            // Same shape as any other reroute: the gate fails (retryable
            // — a human chose a correction lap, not a dead end) and
            // control re-routes; the scheduler's ordinary reroute
            // machinery brings it back to ask again when `target`'s
            // subgraph completes.
            fail(
                ctx,
                node,
                format!("gate chose `{chosen}` — re-routing to `{target}`"),
                true,
            )
            .await?;
            ctx.emit(
                Some(&node.id),
                EventPayload::Node(NodeEvent::Rerouted(
                    yunta_core::events::NodeReroutedPayload::new(
                        target.clone(),
                        yunta_core::events::RerouteCause(yunta_core::events::Failure::message(
                            format!("gate `{}` chose `{chosen}`", node.id),
                        )),
                        yunta_core::events::RerouteOrigin::GateChoice,
                        None,
                        None,
                    ),
                )),
            )
            .await?;
        }
        None => {
            ctx.emit(
                Some(&node.id),
                EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                    chosen.to_string(),
                    TokenUsage::default(),
                ))),
            )
            .await?;
            write_progress(ctx).await?;
        }
    }
    Ok(GateStep::Resolved)
}

async fn emit_started(ctx: &RunCtx<'_>, node: &Node) -> Result<(), RunError> {
    let events = ctx.load_events().await?;
    let attempts = crate::replay::derive(&events)
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

fn encode_ref(published: &PublishedGate) -> String {
    serde_json::to_string(published).unwrap_or_default()
}

fn decode_ref(external_ref: &str) -> Result<PublishedGate, RunError> {
    serde_json::from_str(external_ref).map_err(|e| RunError::Broken {
        diagnostic: format!("gate_waiting.external_ref `{external_ref}` isn't valid: {e}"),
    })
}

/// The escalation object, reused verbatim for the no-forge
/// degradation — two options wide enough to cover every review mapping
/// a human can decide from the console: approve (finishes the node) or
/// reject (fails it, retryable — so a declared `on_failure.goto` still
/// gets a chance, same as a real "changes requested").
async fn degrade_to_console(
    ctx: &RunCtx<'_>,
    node: &Node,
    summary: String,
) -> Result<GateStep, RunError> {
    let escalation = Escalation::new(
        summary.clone(),
        vec![Fact::bare("no forge reachable from this machine")].into(),
        NonEmpty::from((
            offers::approve_from_console(),
            vec![offers::reject_from_console()],
        )),
    )
    .map_err(|source| RunError::Broken {
        diagnostic: format!("node `{}`'s gate: {source}", node.id),
    })?;
    let Some(choice) = ctx.ask_human(&escalation).await? else {
        return Ok(GateStep::Waiting(PauseReason::Escalation(Box::new(
            escalation,
        ))));
    };

    ctx.emit(
        Some(&node.id),
        EventPayload::Gates(GateEvent::Waiting(escalation.into_payload())),
    )
    .await?;
    ctx.emit(
        Some(&node.id),
        EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Chosen(
            choice.clone(),
        ))),
    )
    .await?;
    emit_started(ctx, node).await?;
    if ReservedOption::of(&choice.option) == Some(ReservedOption::Approve) {
        ctx.emit(
            Some(&node.id),
            EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                format!("approved from the console by {}", choice.by),
                TokenUsage::default(),
            ))),
        )
        .await?;
        write_progress(ctx).await?;
    } else {
        fail(ctx, node, "rejected from the console".to_string(), true).await?;
    }
    Ok(GateStep::Resolved)
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
        Err(e) => {
            emit_started(ctx, node).await?;
            fail(ctx, node, e.to_string(), false).await?;
            Ok(Step::Ended(GateStep::Resolved))
        }
    }
}
