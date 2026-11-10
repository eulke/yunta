//! A plan laid out to be read in one screen at a terminal: what it is, the places a person
//! can stand on and what the plan says of itself as a whole, what is being read, and what
//! is being asked. Data: a surface lays it out for a size.

use super::{ChoiceRow, Legend};
use crate::diagram::Diagram;
use crate::doc::Doc;

/// One screen of a plan.
pub struct Desk {
    /// What it is: each part apart by a separator on the first row.
    pub title: Vec<String>,
    /// What stands at the far end of the first row.
    pub edge: Option<String>,
    /// What it is about, in a line.
    pub gist: Option<String>,
    /// What the person asked for, in a line.
    pub asked: Option<String>,
    pub rail: Vec<RailGroup>,
    pub pane: Pane,
    pub foot: DeskFoot,
}

/// Rows of the rail that go together: a surface drops a group whole, and never one with a
/// place to stand on.
pub struct RailGroup {
    pub title: Option<String>,
    pub rows: Vec<RailRow>,
}

/// One thing the rail says.
pub enum RailRow {
    /// A place a person can stand on. `lead` is the step it opens, when it opens one.
    Stop {
        lead: String,
        name: String,
        pointed: bool,
    },
    /// How many of something there are, and the key that shows them, when one does.
    Tally {
        what: String,
        count: usize,
        key: Option<char>,
    },
    /// Something that weighs on the decision.
    Weighs(String),
    /// Two things that go together on a row: a path and what changes it.
    Pair { left: String, right: String },
    /// The order things go in, as chains of names.
    Order(Diagram),
}

/// What is being read.
pub struct Pane {
    pub title: String,
    /// What stands at the far end of its title.
    pub edge: Option<String>,
    /// Where it is among the places a person can stand on, said where no rail says it.
    pub place: Option<String>,
    /// All of it, as a document: each surface lays it out at the pane's measure.
    pub doc: Doc,
    /// The first of its lines, as a surface lays them out, a person asked to see.
    pub top: usize,
    /// Whether the line a pointer stands on has to be in sight, whatever `top` says.
    pub follows_pointer: bool,
}

/// What is being asked, under what is being read.
pub struct DeskFoot {
    pub question: String,
    pub row: FootRow,
    /// The keys it answers to, each with what it does.
    pub legend: Legend,
}

/// What a surface laid out of a desk's pane: the first of its lines it shows, how many
/// lines a page of it holds, and the last first line a scroll can reach. What the person
/// reading scrolls by, handed back to whoever holds the desk rather than kept by the
/// drawing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PaneLaid {
    pub top: usize,
    pub page: usize,
    pub most: usize,
}

/// How the question stands.
pub enum FootRow {
    /// Open, with the options a number picks, in the order their numbers
    /// pick them, and how many more stand on the card past the last number.
    Options { named: Vec<String>, more: usize },
    /// One of them about to be chosen, with what it does and what it costs.
    Confirming(ChoiceRow),
}

#[cfg(test)]
pub(crate) mod tests;
