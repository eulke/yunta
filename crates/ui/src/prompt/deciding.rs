//! A decision with the plan it is on behind it: the card a person answers it on, and
//! the desk the plan is read at, a key away, one of them in front at a time.
//!
//! The turns between the two are the decision's, wherever it is held: asking to read
//! turns to the plan; leaving the plan turns back to the card or, where a person came
//! to read the plan, leaves the decision waiting; a number picked at the desk answers
//! the card with that option. A surface that holds the two alike holds the decision as
//! one [`Answerable`]; a terminal, which reads a plan on its other screen, holds the one
//! [`Deciding::in_front`] and hands back what it came to, through
//! [`Deciding::replied`] and [`Deciding::left`]. Both turn the same way.

use surco::components::PaneLaid;
use surco::doc::Block;
use surco::look::Look;
use yunta_render::escalation::claim_words;
use yunta_render::page::desk::{self, Reading};
use yunta_render::prompt::Escape;

use super::plan_desk::Leaving;
use super::{
    Answerable, Asked, DecisionCard, DecisionPut, Left, PickedOption, PlanDesk, Press, Reply,
    Stroke,
};

/// What a decision opens on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opens {
    /// Its card, with the plan a key away.
    TheCard,
    /// The plan it is on, for a person who came to read it.
    ThePlan,
}

/// The one of a decision's two prompts in front, to be held. Not where a decision
/// opens ([`Opens`]), which is where it stood first.
pub enum InFront<'d> {
    /// The card, answered with one key.
    TheCard(&'d mut DecisionCard),
    /// The desk of the plan the decision is on.
    ThePlan(&'d mut PlanDesk),
}

/// A decision being answered: its card, and the plan it is on when it is on one.
#[derive(Debug, Clone)]
pub struct Deciding {
    card: DecisionCard,
    plan: Option<Plan>,
}

/// The plan a decision is on: the desk it is read at, what the decision opened on, and
/// whether the plan is in front of the card.
#[derive(Debug, Clone)]
struct Plan {
    desk: PlanDesk,
    opens: Opens,
    in_front: bool,
}

impl Deciding {
    /// The decision `put` puts, opening on `opens`, Escape at its card doing what
    /// `escape` says: its card, with the plan behind it where it shows one with a place
    /// to stand on.
    pub fn put(put: &DecisionPut, opens: Opens, escape: Escape) -> Self {
        let reading = put
            .shown
            .as_ref()
            .and_then(|(run, documents)| desk::reading(documents, run));
        let asked = Asked {
            question: claim_words(&put.escalation),
            node: put.node.as_ref().map(ToString::to_string),
            run: put.run().map(|run| run.handle().to_string()),
            called: put.called.clone(),
        };
        let card = DecisionCard::new(put.clone(), escape);
        match reading {
            Some(reading) => Deciding::on_plan(card, reading, asked, opens),
            None => Deciding::new(card),
        }
    }

    /// The decision on `card` alone, with no plan behind it.
    pub(crate) fn new(card: DecisionCard) -> Self {
        Self { card, plan: None }
    }

    /// The decision on `card` with the plan `reading` behind it, read at a desk headed
    /// by `asked`, opening on `opens`. The card offers reading the plan, and the desk
    /// lists the card's options; Escape at the desk goes back to the card, or, for a
    /// person who came to read the plan, does what it does at the card.
    pub(crate) fn on_plan(
        card: DecisionCard,
        reading: Reading,
        asked: Asked,
        opens: Opens,
    ) -> Self {
        let card = card.reading();
        let leaving = match opens {
            Opens::TheCard => Leaving::ToTheCard,
            Opens::ThePlan => Leaving::TheCommand(card.escape()),
        };
        let plan = Plan {
            desk: PlanDesk::new(reading, asked, card.rows(), leaving),
            opens,
            in_front: opens == Opens::ThePlan,
        };
        Self {
            card,
            plan: Some(plan),
        }
    }

    /// The one in front, to be held.
    pub fn in_front(&mut self) -> InFront<'_> {
        match self.plan.as_mut() {
            Some(Plan {
                desk,
                in_front: true,
                ..
            }) => InFront::ThePlan(desk),
            _ => InFront::TheCard(&mut self.card),
        }
    }

    /// What a reply on the card comes to: an option chosen answers the decision, and
    /// asking to read the plan turns to it.
    pub fn replied(&mut self, reply: Reply) -> Option<PickedOption> {
        match reply {
            Reply::Chose(picked) => Some(picked),
            Reply::Read => {
                if let Some(plan) = self.plan.as_mut() {
                    plan.in_front = true;
                }
                None
            }
        }
    }

    /// What leaving the plan comes to, the card in front again: going back to the card,
    /// or, where a person came to read the plan, the decision left waiting; a number
    /// picked answers the card with that option, or opens its field where the option
    /// asks for words; Ctrl-C stops the run.
    pub fn left(&mut self, left: Left) -> Press<PickedOption> {
        self.plan_unshown();
        let came_to_read = self
            .plan
            .as_ref()
            .is_some_and(|plan| plan.opens == Opens::ThePlan);
        match left {
            Left::Back if came_to_read => Press::Decline,
            Left::Back => Press::Moved,
            Left::Picked(index) => match self.card.pick(index) {
                Some(picked) => Press::Answer(picked),
                None => Press::Moved,
            },
            Left::Interrupted => Press::Interrupt,
        }
    }

    /// The decision put to the person.
    pub fn decision(&self) -> &DecisionPut {
        self.card.decision()
    }

    /// Whether the plan stands in front of the card.
    pub fn plan_in_front(&self) -> bool {
        self.desk_in_front().is_some()
    }

    /// The plan has no room where the decision is held: the card stands in front.
    pub fn plan_unshown(&mut self) {
        if let Some(plan) = self.plan.as_mut() {
            plan.in_front = false;
        }
    }

    /// The desk, while the plan is in front.
    fn desk_in_front(&self) -> Option<&PlanDesk> {
        self.plan
            .as_ref()
            .filter(|plan| plan.in_front)
            .map(|plan| &plan.desk)
    }
}

impl Answerable for Deciding {
    type Answer = PickedOption;

    /// A stroke goes to the one in front, and what it comes to turns the decision.
    fn press(&mut self, stroke: Stroke) -> Press<PickedOption> {
        match self.in_front() {
            InFront::ThePlan(desk) => match desk.press(stroke) {
                Press::Answer(left) => self.left(left),
                Press::Stay => Press::Stay,
                Press::Moved => Press::Moved,
                Press::Decline => Press::Decline,
                Press::Interrupt => Press::Interrupt,
            },
            InFront::TheCard(card) => match card.press(stroke) {
                Press::Answer(reply) => match self.replied(reply) {
                    Some(picked) => Press::Answer(picked),
                    None => Press::Moved,
                },
                Press::Stay => Press::Stay,
                Press::Moved => Press::Moved,
                Press::Decline => Press::Decline,
                Press::Interrupt => Press::Interrupt,
            },
        }
    }

    /// Whether the one in front takes keys as words.
    fn typing(&self) -> bool {
        match self.desk_in_front() {
            Some(desk) => desk.typing(),
            None => self.card.typing(),
        }
    }

    /// The one in front, as its block.
    fn block(&self, look: &Look) -> Block {
        match self.desk_in_front() {
            Some(desk) => desk.block(look),
            None => self.card.block(look),
        }
    }

    /// What a surface laid out of the plan's pane, for its scroll keys.
    fn laid(&mut self, pane: PaneLaid) {
        if let Some(plan) = self.plan.as_mut() {
            plan.desk.laid(pane);
        }
    }
}

#[cfg(test)]
mod tests;
