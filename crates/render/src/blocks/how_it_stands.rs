//! How a node stands, as its own scene says it while a person points at it and the run does
//! not lead, and as its printed page prints it: that it has not run yet; why its run leaves
//! it out; what a person's decision holds it on, in the log's words; or, once it ran, the
//! word it stands at, what it took and spent, and what it came to.
//!
//! Each fact is read where another part already decides it: the word is the one the nodes
//! beside the pane list it as (`watched::listed_as`), so a gate that sent the run back
//! stands as rerouted under the mark the list gives it; what it took is the list's measure,
//! a person's time at a gate a person was asked at; what a decision holds it on is the
//! detail its word carries on every surface ([`NodeDisplay`]). A pane short of rows keeps
//! [`HowItStands::least`], what still says how the node stands. A printed page says each
//! fact in the same words, then keeps after them what the screen leaves to it: what a
//! node spent, its tokens in and out ([`HowItStands::printed_fields`]).

use std::time::Duration;

use surco::components::{Fields, Prose};
use surco::doc::Block;
use surco::figures::{duration, Tokens};
use surco::glyphs::Glyphs;
use surco::line::Line;
use surco::words::Standing;
use yunta_core::events::{NodeState, TokenUsage};
use yunta_core::{Because, ModeName};
use yunta_replay::frame::{NodeFrame, NodeStanding};

use crate::spend::Breakdown;
use crate::state::NodeDisplay;
use crate::watched::{listed_as, time_taken};

/// The words over what a node a person's decision holds waits on, and over what a run
/// standing still waits on.
pub(super) const WAITS_ON: &str = "what it waits on";

/// What a node's own page says of a node its run has not come to.
const NOT_RUN: &str = "It has not run yet.";

/// The label of the word a node that ran stands at, and of how a gate's pull request stands.
pub(super) const STANDS: &str = "stands";

/// The label of how long a node that ran took.
pub(super) const TOOK: &str = "took";

/// The label of what a node that ran spent.
const TOKENS: &str = "tokens";

/// How a node stands, as its own scene says it and its printed page prints it. Not replay's
/// `NodeStanding`, where its log puts it, nor Surco's `Standing`, the word it is called by.
pub enum HowItStands {
    /// The run has not come to it.
    NotRun,
    /// The run's mode leaves it out.
    LeftOutByItsMode(ModeName),
    /// The run leaves it out, for why it recorded.
    LeftOut(Because),
    /// A person's decision holds the run at it: what it waits on, in the log's words.
    WaitsOn(String),
    /// It ran: the word it stands at, what it took and spent, and what it came to.
    Ran {
        stands: Standing,
        took: Option<Duration>,
        tokens: TokenUsage,
        came_to: Option<CameTo>,
    },
}

/// What a node that ran came to, in its own words.
pub enum CameTo {
    /// The outcome it finished with, as its session or its check said it.
    Finished(String),
    /// Why it failed, as its log recorded it.
    Failed(String),
}

impl HowItStands {
    /// How `node` stands in a run of the mode `mode`, the run waiting on a person at it where
    /// `waited_at`: what that person's decision holds it on, where its word carries it.
    pub fn of(node: &NodeFrame, mode: &ModeName, waited_at: bool) -> HowItStands {
        if let (true, Some(what)) = (waited_at, NodeDisplay::from(node).modifier) {
            return HowItStands::WaitsOn(what);
        }
        match &node.state {
            NodeStanding::ToGo => HowItStands::NotRun,
            NodeStanding::Skipped => HowItStands::LeftOutByItsMode(mode.clone()),
            NodeStanding::LeftOut(because) => HowItStands::LeftOut(because.clone()),
            NodeStanding::Paused { .. } | NodeStanding::Reached(_) => HowItStands::Ran {
                stands: listed_as(node),
                took: time_taken(node),
                tokens: node.tokens,
                came_to: came_to(&node.state),
            },
        }
    }

    /// What a node's own scene says of it, top to bottom: what a decision holds it on under
    /// its heading; else what it says of itself, then what it says in a sentence.
    pub fn blocks(&self) -> Vec<Block> {
        match self {
            HowItStands::WaitsOn(what) => vec![
                Block::Heading(WAITS_ON.to_string()),
                Block::Lines(vec![Line::new().plain(what.as_str())]),
            ],
            HowItStands::NotRun
            | HowItStands::LeftOutByItsMode(_)
            | HowItStands::LeftOut(_)
            | HowItStands::Ran { .. } => self.ran().into_iter().chain(self.said()).collect(),
        }
    }

    /// What still says how it stands where its pane is short of rows: the word it stands at
    /// alone, once it ran; the sentence that it did not run, whole; nothing where a decision
    /// holds it, which the screen's attention row says.
    pub fn least(&self) -> Vec<Block> {
        match self {
            HowItStands::WaitsOn(_) => Vec::new(),
            HowItStands::Ran { stands, .. } => {
                vec![Fields::new().push_if(STANDS, stands.word()).into()]
            }
            HowItStands::NotRun | HowItStands::LeftOutByItsMode(_) | HowItStands::LeftOut(_) => {
                self.said()
            }
        }
    }

    /// What a node that ran says of itself, labelled: the word it stands at, what it took
    /// where it ran for any time, and what it spent where it spent any. Empty for a node that
    /// did not run or that a decision holds.
    pub fn fields(&self) -> Fields {
        self.labelled(None)
    }

    /// Its [`fields`](HowItStands::fields) as a printed page prints them: what it spent in
    /// the same words, then, after the separator `glyphs` draw, its tokens in and out, which
    /// the screen leaves to the page.
    pub fn printed_fields(&self, glyphs: Glyphs) -> Fields {
        self.labelled(Some(glyphs))
    }

    /// Its fields, each fact the screen leaves to a printed page kept after the separator
    /// `printed` draws, where it is printed.
    fn labelled(&self, printed: Option<Glyphs>) -> Fields {
        match self {
            HowItStands::Ran {
                stands,
                took,
                tokens,
                ..
            } => {
                let spent = match (tokens.total(), printed) {
                    (0, _) => String::new(),
                    (total, None) => Tokens(total).to_string(),
                    (total, Some(glyphs)) => {
                        format!("{} {} {}", Tokens(total), glyphs.sep(), Breakdown(tokens))
                    }
                };
                Fields::new()
                    .push_if(STANDS, stands.word())
                    .push_if(TOOK, took.map(duration).unwrap_or_default())
                    .push_if(TOKENS, spent)
            }
            HowItStands::NotRun
            | HowItStands::LeftOutByItsMode(_)
            | HowItStands::LeftOut(_)
            | HowItStands::WaitsOn(_) => Fields::new(),
        }
    }

    /// What it says in a sentence: that it has not run, why its run leaves it out, or what it
    /// came to. None for a node that came to no sentence yet or that a decision holds.
    pub fn said(&self) -> Vec<Block> {
        match self {
            HowItStands::NotRun => vec![Prose(NOT_RUN.to_string()).into()],
            HowItStands::LeftOutByItsMode(mode) => {
                vec![Prose(format!("The mode `{mode}` leaves it out.")).into()]
            }
            HowItStands::LeftOut(because) => {
                vec![Prose(format!("The run leaves it out: {because}.")).into()]
            }
            HowItStands::Ran {
                came_to: Some(CameTo::Finished(summary)),
                ..
            } => vec![Block::Markdown(summary.clone())],
            HowItStands::Ran {
                came_to: Some(CameTo::Failed(why)),
                ..
            } => vec![Prose(why.clone()).into()],
            HowItStands::Ran { came_to: None, .. } | HowItStands::WaitsOn(_) => Vec::new(),
        }
    }

    /// What it says of itself, as a block, where it ran.
    fn ran(&self) -> Option<Block> {
        match self {
            HowItStands::Ran { .. } => Some(self.fields().into()),
            HowItStands::NotRun
            | HowItStands::LeftOutByItsMode(_)
            | HowItStands::LeftOut(_)
            | HowItStands::WaitsOn(_) => None,
        }
    }
}

/// What a node standing at `state` came to: the outcome it finished with, or why it failed.
/// None for one at work, waiting, or held by its run's pause.
fn came_to(state: &NodeStanding) -> Option<CameTo> {
    match state {
        NodeStanding::Reached(NodeState::Finished { outcome, .. }) => {
            Some(CameTo::Finished(crate::outcome::sentence(outcome)))
        }
        NodeStanding::Reached(NodeState::Failed { failure, .. }) => {
            Some(CameTo::Failed(crate::failure::headline(failure)))
        }
        NodeStanding::Reached(NodeState::Running { .. } | NodeState::Waiting { .. })
        | NodeStanding::Paused { .. }
        | NodeStanding::ToGo
        | NodeStanding::Skipped
        | NodeStanding::LeftOut(_) => None,
    }
}
