//! See [`super`]. An ended run's pane, at the node its end names: under that node's title,
//! the end as render says it. A run that failed says how the node failed, the end of what it
//! printed, its kind, its attempt and how long it worked; one that closed promoted, why, in
//! the log's words, then its successor, once one came or none did; one whose log broke,
//! where the log stops making sense and how far it reads. A run closed after a stop says
//! nothing past its title: its word across the screen says it. No feed closes it: the node's
//! feed is on its own scene, one `j` or `k` away.

use surco::components::{Note, Region};
use surco::doc::{Block, Doc};
use surco::line::Line;
use surco::look::Look;
use yunta_core::events::{Failure, NodeState};
use yunta_core::NodeId;
use yunta_render::blocks::{HowItFailed, ItsSuccessor, WhereItsLogBreaks};
use yunta_replay::frame::{NodeStanding, RunPhase};

use super::super::{Framed, RunStage};
use super::titled;

impl RunStage {
    /// The pane of the run `framed`, which ended, for the node `pointed`, the one its end
    /// names: its title, the end as it ended, `look_failed` said under it where a look failed.
    pub(super) fn ended_pane(
        &self,
        framed: &Framed<'_>,
        pointed: &NodeId,
        look_failed: Option<&String>,
        look: &Look,
    ) -> Region {
        let said = match &framed.frame.phase {
            RunPhase::Failed { failure } => failed(framed, pointed, failure.as_ref()),
            RunPhase::Promoted { .. } => promoted(framed, look),
            RunPhase::Broken { diagnostic } => broken(framed, diagnostic),
            RunPhase::Cancelled
            | RunPhase::Created
            | RunPhase::Running
            | RunPhase::Waiting { .. }
            | RunPhase::Finished => Vec::new(),
        };
        let failed = look_failed.map(|failed| Block::from(Note(failed.clone())));
        let doc = Doc::new().with_all(said).with_all(failed);
        titled(Region::new(doc), framed, pointed)
    }
}

/// How the node `pointed` of the run `framed` failed: the failure it stands failed with, else
/// `failure`, the one the run closed on.
fn failed(framed: &Framed<'_>, pointed: &NodeId, failure: Option<&Failure>) -> Vec<Block> {
    let Some(node) = framed.frame.nodes.iter().find(|node| node.id == *pointed) else {
        return Vec::new();
    };
    let its_own = match &node.state {
        NodeStanding::Reached(NodeState::Failed { failure, .. }) => Some(failure),
        _ => None,
    };
    let Some(failure) = its_own.or(failure) else {
        return Vec::new();
    };
    let workflow = &framed.in_view.run.manifest().workflow;
    let declared = workflow
        .iter_nodes()
        .find(|declared| declared.id == *pointed);
    HowItFailed {
        failure,
        node,
        times: framed.times.node(&node.id),
        declared,
    }
    .blocks()
}

/// Why the run `framed` closed promoted, on a row of its own, then its successor where one
/// came or none did, in `look`'s glyphs.
fn promoted(framed: &Framed<'_>, look: &Look) -> Vec<Block> {
    let state = framed.in_view.run.state();
    let why = state.run.promotion().map(|promotion| {
        let reason = yunta_render::outcome::promotion_reason(
            promotion.cause.as_ref(),
            None,
            &promotion.evidence,
        );
        Block::Rows(vec![Line::new().plain(reason)])
    });
    let successor = framed.in_view.successor.as_deref();
    let framed_at = successor.map(|successor| {
        let run = &successor.run;
        (run.frame(), run.times(framed.now))
    });
    let its_successor = match (&framed_at, framed.in_view.no_successor) {
        (Some((frame, times)), _) => Some(ItsSuccessor::Came(frame, times)),
        (None, true) => Some(ItsSuccessor::NoneCame),
        (None, false) => None,
    };
    let its_successor = its_successor.map(|successor| successor.blocks(look.glyphs));
    why.into_iter()
        .chain(its_successor.into_iter().flatten())
        .collect()
}

/// Where the log of the run `framed` stops making sense, as `diagnostic` says it, and how far
/// it reads.
fn broken(framed: &Framed<'_>, diagnostic: &str) -> Vec<Block> {
    WhereItsLogBreaks {
        diagnostic,
        reads_up_to: framed.in_view.run.state().reads_up_to(),
        waited_on: framed.frame.waits_on_a_person_at(),
    }
    .blocks()
}
