//! See [`super`]. What a scene shows and offers, which the stage reads of it to draw and
//! to act: whether its run's own decision stands in it to answer, the pane it composes,
//! whether a feed closes it, whether the run's nodes stand beside it and whether it heads
//! itself in place of what stands across the run, the place the trail names it by, which of
//! the acts the run offers its legend names, which its keys take of going on and closing, and
//! whether it offers making room or a new run.
//! Each is a `match` on the scene by name.

use super::{Grounds, Scene, Standstill};

/// A pane a scene composes of render's parts of the run in view, for the node pointed at.
/// Not a [`Scene`], what the main area stands as, whose pane this is where it composes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComposedPane {
    /// What a node at work hands over or runs, over its feed.
    Work,
    /// A loop's tasks at work, each with its proof and its scope, over the loop's one feed.
    Board,
    /// The run's findings, at the one pointed at, over the feed of the node that reviews or
    /// fixes them.
    Findings,
    /// How a node the person points at stands, over its feed whole.
    Node,
    /// Why a run stands with nothing to answer here, and what moves it again.
    Standstill,
    /// The pull request a gate waits on, and how it stands as its forge answered.
    PullRequest,
    /// A question the run's process asks at its own terminal, who asks it and since when,
    /// and that nothing here answers it.
    AskedElsewhere,
    /// What a run that finished was held to, spent, changed and opened, and the findings
    /// that block it, over the harvested field.
    Receipt,
    /// How a run ended otherwise, at the node its end names: how it failed, its successor,
    /// or where its log stops making sense.
    Ended,
    /// The decision a run waits on, its card in place at the pane's foot, under what it
    /// rests on.
    Card(Grounds),
    /// A round of questions a run waits on, in place under the request it is about, in the
    /// place of the nodes and the pane.
    Round,
    /// The plan a gate shows, read at its desk a place at a time, the gate's options at its
    /// foot: it heads itself, in the place of what stands across the run, its nodes, the pane
    /// and what the run spent.
    Desk,
}

/// Which of the acts the run offers a scene's legend names among its own: going on, by
/// whatever its resume does, and closing it for good. Not the acts themselves, which the
/// run offers whatever the scene, the keys take and the palette lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NamedActs {
    /// Neither: what the scene shows asks nothing of them.
    Neither,
    /// Going on only by the resume an answer recorded awaits, where no engine takes it up:
    /// work, at work or standing still, which nothing else of the run's moves.
    TheAnswersResume,
    /// Going on alone: what moves the run again is not closing it.
    GoingOn,
    /// Going on and closing.
    Both,
}

impl Scene<'_> {
    /// Whether the run's own decision stands in it, for the person to answer in place: a
    /// round, the desk, a card.
    pub fn asks_in_place(&self) -> bool {
        match self {
            Scene::Round | Scene::Desk | Scene::Card(..) => true,
            Scene::Work
            | Scene::Board
            | Scene::Findings
            | Scene::PullRequest
            | Scene::Standstill(_)
            | Scene::AskedElsewhere
            | Scene::Receipt
            | Scene::Ended
            | Scene::Node => false,
        }
    }

    /// Whether its pane ends with the feed of the node pointed at: the work, the board, the
    /// findings and a node's own scene. A prompt's, a pull request's, a standstill's, a run
    /// asked elsewhere's, the receipt's and an ended run's show none.
    pub(crate) fn ends_with_its_feed(&self) -> bool {
        match self {
            Scene::Work | Scene::Board | Scene::Findings | Scene::Node => true,
            Scene::Round
            | Scene::Desk
            | Scene::Card(..)
            | Scene::PullRequest
            | Scene::Standstill(_)
            | Scene::AskedElsewhere
            | Scene::Receipt
            | Scene::Ended => false,
        }
    }

    /// The pane it composes of render's parts of the run in view for the node pointed at,
    /// whoever points at it: the work scene's, the board's, the findings', a node's own, a
    /// standstill's, a pull request's, a run asked elsewhere's, the receipt's, an ended
    /// run's, a card on what it rests on, a round, and the plan's desk.
    pub(crate) fn composes(&self) -> ComposedPane {
        match self {
            Scene::Card(_, grounds) => ComposedPane::Card(*grounds),
            Scene::Round => ComposedPane::Round,
            Scene::Desk => ComposedPane::Desk,
            Scene::Work => ComposedPane::Work,
            Scene::Board => ComposedPane::Board,
            Scene::Findings => ComposedPane::Findings,
            Scene::Node => ComposedPane::Node,
            Scene::Standstill(_) => ComposedPane::Standstill,
            Scene::PullRequest => ComposedPane::PullRequest,
            Scene::AskedElsewhere => ComposedPane::AskedElsewhere,
            Scene::Receipt => ComposedPane::Receipt,
            Scene::Ended => ComposedPane::Ended,
        }
    }

    /// Whether the run's nodes stand beside its pane: on every scene but a round and the
    /// plan's desk, which take their place too.
    pub(crate) fn lists_its_nodes(&self) -> bool {
        match self {
            Scene::Round | Scene::Desk => false,
            Scene::Work
            | Scene::Board
            | Scene::Findings
            | Scene::Card(..)
            | Scene::PullRequest
            | Scene::Standstill(_)
            | Scene::AskedElsewhere
            | Scene::Receipt
            | Scene::Ended
            | Scene::Node => true,
        }
    }

    /// Whether it heads itself and takes every row between the rule and the attention row:
    /// the plan's desk, whose head says the run, its plan and what the person asked for, so
    /// nothing of the run stands across it and no meter under it.
    pub(crate) fn heads_itself(&self) -> bool {
        match self {
            Scene::Desk => true,
            Scene::Work
            | Scene::Board
            | Scene::Findings
            | Scene::Round
            | Scene::Card(..)
            | Scene::PullRequest
            | Scene::Standstill(_)
            | Scene::AskedElsewhere
            | Scene::Receipt
            | Scene::Ended
            | Scene::Node => false,
        }
    }

    /// The place it stands as on the trail, after the run's handle, where it names one: the
    /// receipt, which is the run's whole and stands on no node. None for every other scene,
    /// where the trail names the node pointed at.
    pub(crate) fn place(&self) -> Option<&'static str> {
        match self {
            Scene::Receipt => Some(yunta_render::screen::RECEIPT),
            Scene::Work
            | Scene::Board
            | Scene::Findings
            | Scene::Round
            | Scene::Desk
            | Scene::Card(..)
            | Scene::PullRequest
            | Scene::Standstill(_)
            | Scene::AskedElsewhere
            | Scene::Ended
            | Scene::Node => None,
        }
    }

    /// Which of the acts the run offers here its legend names among its own, and so which
    /// of going on and closing its keys take (`r`, `x`), the palette listing the others with
    /// no key: none on a node's own scene, which asks nothing, the run's decision being one
    /// `a` or `f` away, nor on a pull request's, which a forge's review moves, the pull
    /// request one `o` away, nor on a prompt in place, whose legend is the prompt's own; on a
    /// node's work, at work or standing still, only the resume an answer recorded awaits, its
    /// node's work saying what else holds it; going on alone on a run standing still for room
    /// or on what is left half done, which making room or checking moves again; both
    /// elsewhere.
    pub(crate) fn named_acts(&self) -> NamedActs {
        match self {
            Scene::Node | Scene::PullRequest | Scene::Round | Scene::Desk | Scene::Card(..) => {
                NamedActs::Neither
            }
            Scene::Work | Scene::Board | Scene::Findings => NamedActs::TheAnswersResume,
            Scene::Standstill(Standstill::NoRoom | Standstill::HalfDone) => NamedActs::GoingOn,
            Scene::Standstill(
                Standstill::Unanswered | Standstill::Stopped | Standstill::NoProcess,
            )
            | Scene::AskedElsewhere
            | Scene::Receipt
            | Scene::Ended => NamedActs::Both,
        }
    }

    /// Whether it offers making room for the run, upkeep opened over it: on a run standing
    /// still for room, where collecting what runs left makes it.
    pub(crate) fn makes_room(&self) -> bool {
        match self {
            Scene::Standstill(Standstill::NoRoom) => true,
            Scene::Standstill(
                Standstill::Unanswered
                | Standstill::HalfDone
                | Standstill::Stopped
                | Standstill::NoProcess,
            )
            | Scene::Work
            | Scene::Board
            | Scene::Findings
            | Scene::Round
            | Scene::Desk
            | Scene::Card(..)
            | Scene::PullRequest
            | Scene::AskedElsewhere
            | Scene::Receipt
            | Scene::Ended
            | Scene::Node => false,
        }
    }

    /// Whether its legend names `n`, a new run: on a run's receipt, where what the person
    /// does next is another run. `n` starts one wherever no prompt stands; the legend says
    /// so where the scene comes to it.
    pub(crate) fn names_a_new_run(&self) -> bool {
        match self {
            Scene::Receipt => true,
            Scene::Work
            | Scene::Board
            | Scene::Findings
            | Scene::Round
            | Scene::Desk
            | Scene::Card(..)
            | Scene::PullRequest
            | Scene::Standstill(_)
            | Scene::AskedElsewhere
            | Scene::Ended
            | Scene::Node => false,
        }
    }
}
