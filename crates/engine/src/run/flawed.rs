//! A plan that cannot be proven as it is written, at the gate that would
//! put it before a person: no gate offers to go on with one. A gate
//! answered here withholds the options that would; one answered on a
//! forge is never published, and sends the plan back the way a request
//! for changes does.

use yunta_core::events::{FindingSeverity, Shown};
use yunta_core::shown::{Flaw, ShownContent, ShownDocument};
use yunta_core::{Location, Node, RelativePath};

use super::gate_exec::{emit_started, GateStep};
use super::node_close::fail;
use super::{RunCtx, RunError};

/// Everything that keeps a plan among `documents` from being proven as
/// it is written, each with the plan it is in.
pub(super) fn unprovable(documents: &[ShownDocument]) -> Vec<(&Shown, Flaw)> {
    documents
        .iter()
        .flat_map(|document| match &document.content {
            ShownContent::Tasks(review) => review
                .flaws()
                .into_iter()
                .map(|flaw| (&document.shown, flaw))
                .collect(),
            _ => Vec::new(),
        })
        .collect()
}

/// Why a gate does not go on with what `flaws` were found in; `None`
/// when nothing was.
pub(super) fn withheld_because<'a>(flaws: impl IntoIterator<Item = &'a Flaw>) -> Option<String> {
    let mut flaws = flaws.into_iter();
    let first = flaws.next()?;
    Some(match flaws.count() {
        0 => format!("the plan cannot be proven as it is written: {first}"),
        more => format!("the plan cannot be proven as it is written: {first} (and {more} more)"),
    })
}

/// Sends a plan back from an external gate instead of publishing it: one
/// finding per flaw, on the plan, for the session that rewrites it, and
/// the gate fails retryable, so its `on_failure` takes the run where a
/// request for changes would.
pub(super) async fn send_back(
    ctx: &RunCtx<'_>,
    node: &Node,
    flaws: &[(&Shown, Flaw)],
) -> Result<GateStep, RunError> {
    emit_started(ctx, node).await?;
    let attempt = ctx
        .run_view()
        .await?
        .state
        .nodes
        .get(&node.id)
        .map_or(0, |record| record.attempts);
    for (at, (plan, flaw)) in flaws.iter().enumerate() {
        ctx.engine_finding(
            Some(&node.id),
            &format!("{}-unprovable-{attempt}-{at}", node.id),
            FindingSeverity::Blocking,
            flaw.to_string(),
            Location::run(
                RelativePath::of([crate::artifacts::shown::view_of(plan)]),
                None,
            ),
            format!("{} — {}", flaw.so(), flaw.fix()),
        )
        .await?;
    }
    let because = withheld_because(flaws.iter().map(|(_, flaw)| flaw)).unwrap_or_default();
    fail(
        ctx,
        node,
        format!("not published, and sent back: {because}"),
        true,
    )
    .await?;
    Ok(GateStep::Resolved)
}
