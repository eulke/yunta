//! The blocks every surface is composed of.
//!
//! A surface says, in order, what happened, what needs a person, the
//! evidence, the detail and what to type next. Each of those is a block
//! here — what it says, as data — and laid out once for a terminal's
//! [`Look`]: two surfaces that show a decision show the same block, so a
//! reader who learned it on one reads it on the next. A block with
//! nothing to say draws nothing — no heading over an empty list, no row
//! with an empty value.

mod checklist;
mod decision;
pub mod diagnostic;
mod document;
mod evidence;
mod failure;
mod fields;
mod headline;
mod next;
mod node_table;
mod table;

pub use checklist::{Check, Checklist, Found};
pub use decision::{Chosen, Decision, DecisionOption};
pub use document::{Code, Marked, Prose, Section};
pub use evidence::{Evidence, Whole};
pub use failure::{FailureDetail, FailureSays};
pub use fields::Fields;
pub use headline::Headline;
pub use next::Next;
pub use node_table::{NodeRow, NodeTable};
pub use table::{Cell, Column, Holds, Row, Table};

use super::ink::Line;
use super::Look;

/// How a block is laid out on a terminal: the lines it takes on a stream
/// with `look`. The [`Terminal`](crate::surface::Terminal) surface draws a
/// [`Doc`](crate::doc::Doc) through it; another medium draws the same
/// blocks its own way.
pub trait Drawn {
    fn lines(&self, look: &Look) -> Vec<Line>;
}

#[cfg(test)]
mod tests;
