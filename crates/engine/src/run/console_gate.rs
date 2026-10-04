//! An external gate resolved at this console because no forge is
//! reachable: nothing is ever recorded as published while degraded — no
//! `gate_waiting` without a resolved answer alongside it — so a
//! still-unresolved degraded gate asks fresh on every wake rather than
//! remembering a decision that was never really made.

use yunta_core::events::{
    Escalation, EventPayload, Fact, GateEvent, GateResolvedPayload, PauseReason, Shown, TokenUsage,
    Withheld,
};
use yunta_core::{ExternalGate, Node, NodeKind, NonEmpty};

use super::gate_exec::{emit_started, GateStep};
use super::node_close::{fail, finish_node};
use super::{RunCtx, RunError};
use crate::reserved::{offers, ReservedOption};

/// The escalation object, reused verbatim for the no-forge
/// degradation — two options wide enough to cover every review mapping
/// a human can decide from the console: approve (finishes the node) or
/// reject (fails it, retryable — so a declared `on_failure.goto` still
/// gets a chance, same as a real "changes requested"). A plan among what
/// it would publish that cannot be proven withholds approve.
pub(super) async fn degrade_to_console(
    ctx: &RunCtx<'_>,
    node: &Node,
    summary: String,
) -> Result<GateStep, RunError> {
    let escalation = asked(ctx, node, summary).await?;
    let Some(choice) = ctx.ask_human(Some(&node.id), None, &escalation).await? else {
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
        finish_node(
            ctx,
            node,
            format!("approved from the console by {}", choice.by),
            TokenUsage::default(),
        )
        .await?;
    } else {
        fail(ctx, node, "rejected from the console".to_string(), true).await?;
    }
    Ok(GateStep::Resolved)
}

/// What the console asks: approve or reject, with approve withheld when
/// a plan among what the gate would publish cannot be proven.
async fn asked(ctx: &RunCtx<'_>, node: &Node, summary: String) -> Result<Escalation, RunError> {
    let because = match &node.kind {
        NodeKind::Gate {
            external: Some(external),
            ..
        } => unprovable_held(ctx, external).await?,
        _ => None,
    };
    let escalation = Escalation::new(
        summary,
        vec![Fact::bare("no forge reachable from this machine")].into(),
        NonEmpty::from((
            offers::approve_from_console(),
            vec![offers::reject_from_console()],
        )),
    )
    .map_err(|source| RunError::Broken {
        diagnostic: format!("node `{}`'s gate: {source}", node.id),
    })?;
    let withheld = because.map(|because| Withheld {
        option: offers::approve_from_console().id,
        because,
    });
    Ok(escalation.withholding(withheld.into_iter().collect()))
}

/// Why what an external gate would publish cannot be gone on with, read
/// from the artifacts of it the run holds; `None` when nothing keeps it.
async fn unprovable_held(
    ctx: &RunCtx<'_>,
    external: &ExternalGate,
) -> Result<Option<String>, RunError> {
    let state = ctx.run_view().await?.state;
    let shows: Vec<Shown> = external
        .artifacts
        .iter()
        .filter_map(|spec| {
            state
                .artifacts
                .latest(&yunta_core::events::ArtifactId::from(spec), None)
        })
        .map(|held| Shown {
            producer: held.producer.clone(),
            artifact: held.artifact.clone(),
            content_hash: held.content_hash.clone(),
        })
        .collect();
    let documents = crate::artifacts::shown::documents(ctx.run_dir, &shows, &state).await?;
    let flaws = super::flawed::unprovable(&documents);
    Ok(super::flawed::withheld_because(
        flaws.iter().map(|(_, flaw)| flaw),
    ))
}
