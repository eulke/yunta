//! See [`super`]. A card's pane, where a decision stands in place: under the title of the
//! node it waits at, its far end naming the one task of a loop the decision is about where it
//! is about one; which run bore the run shown, where the stage shows a child in its parent's
//! place; what the decision rests on, by its grounds; then the card at the pane's foot
//! (`in_place`). No feed closes it: the node's feed is on its own scene.
//!
//! What a decision rests on is render's, for what its menu and its failure's cause ask:
//! a gate's, what it shows as its page draws it over a card in place, with the change by
//! file and the findings and the criteria in a line; a failure's, how the node failed, the
//! end of what it printed with what the node is; a scope's, the writes the fence refused
//! that the request asks to make, the task's reason standing in the card under its question;
//! a departure's, what the plan decided and what the session says instead; a cap's, what was
//! spent against it.
//!
//! What stands above the card gives way from its top where the rows are too few, and scrolls;
//! a failure's says itself in fewer rows first, what the node printed cut from its top, and a
//! departure's leaves out why. The card's options never give way to it, as Surco lays a card
//! out at a pane's foot.

use surco::components::{Note, Region};
use surco::doc::{Block, Doc};
use surco::line::Line;
use surco::look::Look;
use yunta_api::WaitingDecision;
use yunta_core::events::Menu;
use yunta_core::{NodeId, RunId};
use yunta_render::blocks::{Departures, FenceRefused, HowItFailed, SpentAgainstItsCap};
use yunta_render::page::{Gate, Page};

use super::super::scene::Grounds;
use super::super::{Framed, RunStage};
use super::title_of;

impl RunStage {
    /// The pane of the decision the run `framed` waits on at the node `pointed`, resting on
    /// `grounds`, its card at the pane's foot: its title, the task the decision is about at
    /// its far end where it is about one; which run bore the run shown, where it is a child
    /// shown in its parent's place; what the decision rests on; `look_failed` said under it
    /// where a look failed; then the card.
    pub(super) fn card_pane(
        &self,
        framed: &Framed<'_>,
        (pointed, grounds): (&NodeId, Grounds),
        look_failed: Option<&String>,
        look: &Look,
    ) -> Region {
        let decision = framed.in_view.awaited_decision();
        let above = |rests_on: Vec<Block>| -> Vec<Block> {
            let bore_it = self
                .child_of()
                .map(|said| Block::Rows(vec![Line::new().plain(said)]));
            let failed = look_failed.map(|failed| Block::from(Note(failed.clone())));
            bore_it.into_iter().chain(rests_on).chain(failed).collect()
        };
        let (rests_on, abridged) = match decision {
            Some(decision) => rests_on((&self.run, framed), decision, grounds, look),
            None => (Vec::new(), Vec::new()),
        };
        let doc = Doc::new()
            .with_all(above(rests_on))
            .with_all(self.prompt_in_sight(framed, look));
        let abridged = abridged.into_iter().map(above).collect();
        let region = Region::new(doc).abridged(abridged);
        let task = decision
            .and_then(|decision| yunta_render::escalation::task_it_is_about(&decision.escalation));
        let title = title_of(framed, pointed).map(|title| match task {
            Some(task) => title.about_the_task(task),
            None => title,
        });
        match title {
            Some(title) => title.titled(region),
            None => region,
        }
    }
}

/// What `decision`, which the run `run`, framed as `framed`, waits on, rests on by
/// `grounds`, in `look`, and what it says in fewer rows over a card short of them.
fn rests_on(
    (run, framed): (&RunId, &Framed<'_>),
    decision: &WaitingDecision,
    grounds: Grounds,
    look: &Look,
) -> (Vec<Block>, Vec<Vec<Block>>) {
    let escalation = &decision.escalation;
    match grounds {
        Grounds::Shown => (shown(run, framed, decision, look), Vec::new()),
        Grounds::Failure => failed(framed, decision),
        Grounds::Scope => {
            let node = decision.on.node();
            let record = node.and_then(|node| framed.in_view.run.state.nodes.get(node));
            let refused = record.map_or(&[][..], |record| record.refused.as_slice());
            let fence = FenceRefused::asked_by(escalation, refused);
            (fence.blocks(look.glyphs), Vec::new())
        }
        Grounds::Departure => {
            let departures = Departures::asked_by(escalation);
            let abridged = departures.abridged(look.glyphs);
            (departures.blocks(look.glyphs), abridged)
        }
        Grounds::Cap => {
            let spent = SpentAgainstItsCap::asked_by(escalation);
            let blocks = spent.map(|spent| spent.blocks()).unwrap_or_default();
            (blocks, Vec::new())
        }
    }
}

/// What a gate's `decision`, which the run `run`, framed as `framed`, waits on, shows over
/// its card in place, in `look`: its documents, the change by file, and the findings and the
/// criteria in a line.
fn shown(run: &RunId, framed: &Framed<'_>, decision: &WaitingDecision, look: &Look) -> Vec<Block> {
    let state = &framed.in_view.run.state;
    let findings = yunta_replay::receipt::findings_summary(state);
    let criteria = yunta_replay::receipt::criteria_summary(state);
    let shown = Gate::OverTheCardInPlace {
        shown: (run, &decision.shown),
        changed: decision.changed.as_ref(),
        findings: &findings,
        criteria: &criteria,
    };
    shown.document(look).into_blocks()
}

/// How the node a failure's `decision`, which the run `framed` waits on, is about failed, and
/// what it says in fewer rows: nothing where the decision is no failure's or names no node
/// the run holds.
fn failed(framed: &Framed<'_>, decision: &WaitingDecision) -> (Vec<Block>, Vec<Vec<Block>>) {
    let failure = match decision.escalation.menu() {
        Some(Menu::Failure(menu)) => Some(&menu.failure),
        _ => None,
    };
    let node = decision.on.node();
    let at = node.and_then(|node| framed.frame.nodes.iter().find(|at| at.id == *node));
    let Some((failure, at)) = failure.zip(at) else {
        return (Vec::new(), Vec::new());
    };
    let workflow = &framed.in_view.run.manifest.workflow;
    let failed = HowItFailed {
        failure,
        node: at,
        declared: workflow.iter_nodes().find(|declared| declared.id == at.id),
    };
    (failed.blocks(), failed.abridged())
}
