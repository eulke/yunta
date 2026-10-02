//! The blocks every surface is composed of.
//!
//! A surface says, in order, what happened, what needs a person, the
//! evidence, the detail and what to type next. Each of those is a block
//! here, laid out once for a stream's [`Look`]: two surfaces that show a
//! decision show the same block, so a reader who learned it on one reads
//! it on the next. A block with nothing to say draws nothing — no
//! heading over an empty list, no row with an empty value.

mod checklist;
mod decision;
pub mod diagnostic;
mod evidence;
mod failure;
mod fields;
mod headline;
mod next;
mod node_table;

pub use checklist::{Check, Checklist, Found};
pub use decision::{Decision, DecisionOption};
pub use evidence::{Evidence, Whole};
pub use failure::FailureDetail;
pub use fields::Fields;
pub use headline::Headline;
pub use next::Next;
pub use node_table::{NodeRow, NodeTable};

use super::ink::Line;
use super::Look;

/// A part of a surface: the lines it takes on a stream with `look`.
pub trait Block {
    fn lines(&self, look: &Look) -> Vec<Line>;
}

/// `blocks`, one after another, painted for `look`, every line ended.
pub fn paint(blocks: &[&dyn Block], look: &Look) -> String {
    blocks
        .iter()
        .flat_map(|block| block.lines(look))
        .map(|line| format!("{}\n", look.ink.paint(&line)))
        .collect()
}

#[cfg(test)]
mod tests;
