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
//!
//! A [`Screen`] is no block: it holds documents in its regions, with what
//! needs the person and the keys, and each interface lays it out its own
//! way. The keys a card, a desk or a screen answers to are one [`Legend`],
//! so a key is said one way wherever a person presses it.

mod attention;
mod card;
mod checklist;
mod choice_list;
mod concern;
mod decision;
mod desk;
mod diffstat;
mod document;
mod evidence;
mod fields;
mod headline;
mod legend;
mod next;
mod node_table;
mod page;
mod problem;
mod ribbon;
mod runs;
mod said;
mod screen;
mod table;
mod typed_answer;

pub use attention::{Answer, Attention};
pub use card::{Card, CardBody, ChoiceRow, Choices, RoundRow, RoundStanding, TypedRow};
pub use checklist::{Check, Checklist, Fix, Found};
pub use choice_list::ChoiceList;
pub use concern::Concern;
pub use decision::{Chosen, Decision, DecisionOption};
pub use desk::{Desk, DeskFoot, FootRow, Pane, PaneLaid, RailGroup, RailRow};
pub use diffstat::{Diffstat, FileRow, MoreFiles, COLUMNS};
pub use document::{Against, Code, Fold, Lead, Marked, Marks, Note, Part, Prose, Section, Title};
pub use evidence::{Evidence, Whole};
pub use fields::{Fields, Reads};
pub use headline::Headline;
pub use legend::{Binding, Keys, Legend};
pub use next::Next;
pub use node_table::{Branch, NodeRow, NodeTable};
pub use page::{Furrow, Gist};
pub use problem::{Place, Problem, Refusal, Severity};
pub use ribbon::{Ribbon, Stretch};
pub use runs::{Edge, RunLine, Runs};
pub use said::{PathNote, Paths, Said};
pub use screen::{Count, Overlay, Region, Screen, Side};
pub use table::{Cell, Column, Holds, Row, Table};
pub use typed_answer::TypedAnswer;

#[cfg(test)]
mod catalog;
#[cfg(test)]
mod tests;
