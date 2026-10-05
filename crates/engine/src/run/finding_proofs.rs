//! A finding a node answered fixed, held to the evidence it proposed: as
//! the node closes, the criterion the finding proposes runs on the tree
//! the node leaves, and the result goes on the log. Passing settles the
//! finding; failing is kept, so a person reads that the answer was not
//! borne out.

use yunta_core::events::{
    EventPayload, FindingAnswer, FindingEvent, FindingProvedPayload, StoredEvent,
};
use yunta_core::{FindingId, Node, NodeId};

use super::{RunCtx, RunError};

/// Proves, on the run's tree as `node` leaves it, every finding standing
/// and unsettled that `node` answered fixed and that proposes a
/// criterion. A command the project's permissions deny is left unrun.
pub(super) async fn prove(ctx: &RunCtx<'_>, node: &Node) -> Result<(), RunError> {
    let owed = owed(&ctx.load_events().await?, &node.id);
    let permissions = ctx.manifest.config.permissions.as_ref();
    for (of, id, cmd) in owed {
        if crate::permissions::command_violation(&cmd, permissions).is_some() {
            continue;
        }
        let run =
            crate::task_cycle::probe_command(&cmd, ctx.worktree, &ctx.memo, ctx.root_supervision())
                .await?;
        let result = crate::task_cycle::to_results(std::slice::from_ref(&run)).remove(0);
        ctx.emit(
            Some(&node.id),
            EventPayload::Findings(FindingEvent::Proved(FindingProvedPayload {
                node: of,
                id,
                result,
            })),
        )
        .await?;
    }
    Ok(())
}

/// Each finding `answerer` answered fixed that stands, is unsettled and
/// proposes a criterion: the node that reported it, its id, the command.
fn owed(events: &[StoredEvent], answerer: &NodeId) -> Vec<(NodeId, FindingId, String)> {
    yunta_core::events::findings::FindingLedger::of(events)
        .standing()
        .findings
        .into_iter()
        .filter(|standing| standing.settled.is_none())
        .filter(|standing| {
            standing.answers.iter().any(|answer| {
                answer.by.as_ref() == Some(answerer) && answer.answer == FindingAnswer::Fixed
            })
        })
        .filter_map(|standing| {
            let cmd = standing.finding.proposed_criterion?.cmd;
            Some((standing.node?, standing.finding.id, cmd))
        })
        .collect()
}
