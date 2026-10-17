//! An external gate resolved at this console because no forge is
//! reachable: nothing is ever recorded as published while degraded — no
//! `gate_waiting` without a resolved answer alongside it — so a
//! still-unresolved degraded gate asks fresh on every wake rather than
//! remembering a decision that was never really made.

use yunta_core::events::{
    Approval, Claim, ConsoleGate, EngineFact, Escalation, EventPayload, Failure, GateEvent,
    GateOption, GateResolvedPayload, Menu, NodeOutcome, PauseReason, Shown, TokenUsage,
    WhyWithheld, Withheld,
};
use yunta_core::reserved::ReservedOption;
use yunta_core::shown::Flaw;
use yunta_core::{ExternalGate, Node, NodeKind, NonEmpty};

use super::gate_exec::{emit_started, GateStep};
use super::node_close::{fail, finish_node};
use super::{RunCtx, RunError};

/// The escalation object, reused verbatim for the no-forge
/// degradation — two options wide enough to cover every review mapping
/// a human can decide from the console: approve (finishes the node) or
/// reject (fails it, so a declared `on_failure.goto` still gets a
/// chance, same as a real "changes requested"). A plan among what
/// it would publish that cannot be proven withholds approve.
pub(super) async fn degrade_to_console(
    ctx: &RunCtx<'_>,
    node: &Node,
    summary: Claim,
) -> Result<GateStep, RunError> {
    let escalation = asked(ctx, node, summary).await?;
    let Some(choice) = ctx.ask_human(Some(&node.id), None, &escalation).await? else {
        return Ok(GateStep::Waiting(PauseReason::Escalation(Box::new(
            escalation,
        ))));
    };

    ctx.emit(
        Some(&node.id),
        EventPayload::Gates(GateEvent::Waiting(Box::new(escalation.into_payload()))),
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
            NodeOutcome::Fact(EngineFact::Approved {
                by: choice.by.clone(),
                on: Approval::Console,
            }),
            TokenUsage::default(),
        )
        .await?;
    } else {
        let failure = Failure::Rejected {
            rejected_by: choice.by.clone(),
            on: Approval::Console,
        };
        fail(ctx, node, failure).await?;
    }
    Ok(GateStep::Resolved)
}

/// What the console asks: approve or reject, with approve withheld when
/// a plan among what the gate would publish cannot be proven.
async fn asked(ctx: &RunCtx<'_>, node: &Node, summary: Claim) -> Result<Escalation, RunError> {
    let flaws = match &node.kind {
        NodeKind::Gate {
            external: Some(external),
            ..
        } => unprovable_held(ctx, external).await?,
        _ => None,
    };
    let escalation = Escalation::new(
        summary,
        Menu::Console(ConsoleGate {}),
        NonEmpty::from((
            GateOption::reserved(ReservedOption::Approve),
            vec![GateOption::reserved(ReservedOption::Reject)],
        )),
    );
    let withheld = flaws.map(|flaws| Withheld {
        option: ReservedOption::Approve.id(),
        because: WhyWithheld::Unprovable { flaws },
    });
    Ok(escalation.withholding(withheld.into_iter().collect()))
}

/// Why what an external gate would publish cannot be gone on with, read
/// from the artifacts of it the run holds; `None` when nothing keeps it.
async fn unprovable_held(
    ctx: &RunCtx<'_>,
    external: &ExternalGate,
) -> Result<Option<NonEmpty<Flaw>>, RunError> {
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
    let flaws: Vec<Flaw> = super::flawed::unprovable(&documents)
        .into_iter()
        .map(|(_, flaw)| flaw)
        .collect();
    Ok(NonEmpty::new(flaws))
}
