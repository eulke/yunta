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
//!
//! A [`Feed`] is what happens at a node, a row a moment, as a pane and a
//! printed page show it: the rows in sight, the newest last, and how many
//! stand above and under them. A row is cut at the edge, never folded and
//! never inside a number, so it stays one moment.
//!
//! A [`Meter`] is what a run has spent, on one row: its count, one bar a
//! node, and its times. Where the row passes the edge its parts go whole,
//! the bars first, so no figure in it is cut.

mod at_a_glance;
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
mod feed;
mod fields;
mod finding_list;
mod form;
mod headline;
mod horizon;
mod in_ascii;
mod key_list;
mod labelled;
mod legend;
mod meter;
mod next;
mod node_table;
mod page;
mod problem;
mod ribbon;
mod runs;
mod said;
mod screen;
mod scroll;
mod survey;
mod table;
mod typed_answer;
mod word_choice;
mod wordmark;

pub use at_a_glance::AtAGlance;
pub use attention::{Answer, Attention};
pub use card::{Card, CardBody, ChoiceRow, Choices, RoundRow, RoundStanding, TypedRow};
pub use checklist::{Check, Checklist, Fix, Found};
pub use choice_list::ChoiceList;
pub use concern::Concern;
pub use decision::{Chosen, Decision, DecisionOption};
pub use desk::{Desk, DeskFoot, FootRow, Pane, RailGroup, RailRow};
pub use diffstat::{Diffstat, FileRow, MoreFiles, COLUMNS};
pub use document::{Against, Code, Fold, Lead, Marked, Marks, Note, Part, Prose, Section, Title};
pub use evidence::{Evidence, Whole};
pub use feed::{Feed, FeedRow};
pub use fields::{Fields, Reads};
pub use finding_list::{FindingList, FindingRow};
pub use form::{Form, FormFoot, FormRow, FormValue};
pub use headline::Headline;
pub use horizon::Horizon;
pub use key_list::KeyList;
pub use labelled::{Labelled, LabelledRow};
pub use legend::{Binding, Keys, Legend};
pub use meter::{Meter, Time};
pub use next::{Next, SameAtAShell};
pub use node_table::{Branch, NodeRow, NodeTable};
pub use page::{Furrow, Gist};
pub use problem::{Place, Problem, Refusal, Severity};
pub use ribbon::{Ribbon, Stretch};
pub use runs::{Edge, RunLine, Runs};
pub use said::{PathNote, Paths, Said};
pub use screen::{Count, Measure, Noted, Overlay, Region, Screen, ScreenLaid, Side, SideAt, Tally};
pub use scroll::{Laid, Sight};
pub use survey::{Survey, SurveyList, SurveyRow, SurveyValue, TypedInPlace};
pub use table::{Cell, Column, Holds, Row, Table};
pub use typed_answer::TypedAnswer;
pub use word_choice::WordChoice;
pub use wordmark::Wordmark;

#[cfg(test)]
pub(crate) mod catalog;
#[cfg(test)]
mod tests;
