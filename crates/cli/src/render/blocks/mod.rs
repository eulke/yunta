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
pub(crate) mod diagnostic;
mod evidence;
mod failure;
mod fields;
mod headline;
mod next;
mod node_table;

pub(crate) use checklist::{Check, Checklist, Found};
pub(crate) use decision::{Decision, DecisionOption};
pub(crate) use evidence::Evidence;
pub(crate) use failure::FailureDetail;
pub(crate) use fields::Fields;
pub(crate) use headline::Headline;
pub(crate) use next::Next;
pub(crate) use node_table::{NodeRow, NodeTable};

use super::ink::Line;
use super::Look;

/// A part of a surface: the lines it takes on a stream with `look`.
pub(crate) trait Block {
    fn lines(&self, look: &Look) -> Vec<Line>;
}

/// `blocks`, one after another, painted for `look`, every line ended.
pub(crate) fn paint(blocks: &[&dyn Block], look: &Look) -> String {
    blocks
        .iter()
        .flat_map(|block| block.lines(look))
        .map(|line| format!("{}\n", look.ink.paint(&line)))
        .collect()
}

#[cfg(test)]
mod tests;
