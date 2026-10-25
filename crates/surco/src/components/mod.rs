//! The blocks every surface is composed of.
//!
//! A surface says, in order, what happened, what needs a person, the
//! evidence, the detail and what to type next. Each of those is a block
//! here — what it says and the role each part plays, as data, never a
//! width, an escape or a glyph — and each surface lays it out its own
//! way: two surfaces that show a decision show the same block, so a
//! reader who learned it on one reads it on the next. A block with
//! nothing to say draws nothing — no heading over an empty list, no row
//! with an empty value.

mod checklist;
mod concern;
mod decision;
mod document;
mod evidence;
mod fields;
mod headline;
mod next;
mod node_table;
mod page;
mod problem;
mod runs;
mod said;
mod table;

pub use checklist::{Check, Checklist, Fix, Found};
pub use concern::Concern;
pub use decision::{Chosen, Decision, DecisionOption};
pub use document::{Against, Code, Lead, Marked, Marks, Note, Part, Prose, Section};
pub use evidence::{Evidence, Whole};
pub use fields::{Fields, Reads};
pub use headline::Headline;
pub use next::Next;
pub use node_table::{NodeRow, NodeTable};
pub use page::{Furrow, Gist};
pub use problem::{Place, Problem, Refusal, Severity};
pub use runs::{Edge, RunLine, Runs};
pub use said::{PathNote, Paths, Said};
pub use table::{Cell, Column, Holds, Row, Table};

#[cfg(test)]
mod catalog;
#[cfg(test)]
mod tests;
