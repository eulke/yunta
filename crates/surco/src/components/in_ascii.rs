//! Every text a screen holds, in ASCII: what a full-screen terminal drawn in the ASCII glyphs
//! lays out ([`Screen::in_ascii`]), each text in the form the glyphs' rule gives it, before
//! any of it is measured.
//!
//! Each kind of part is taken apart once, whole: a struct field by field, put back with each
//! field in ASCII, and an enum variant by variant, so a field or a variant added to a component
//! and left out here is a type error, never a text that slips past the rule; a line, a
//! document and a place in a quoted line, whose parts are their own, are taken apart beside
//! their types, as whole. A word Yunta names in its own source, a `&'static str`, is written in
//! ASCII and stays as written: one outside ASCII fails a test build that folds it. What holds no
//! text is as it was. A cell's id, which marks bytes of its text, is marked again where the
//! ASCII form of that text moved them; a place in a quoted line keeps the line and column it
//! was recorded at, and the marks under its fault stand where the fault's ASCII form does. Not
//! what Markdown, HTML or a printed page draw: those keep every character a screen holds.

use super::fields::Field;
use super::*;
use crate::diagram::{Diagram, Direction, Flowchart, Group, Link, Node, Shape, Stroke};
use crate::doc::Block;
use crate::glyphs::{InAscii, Mark};
use crate::line::{Fill, Span, Tone};
use crate::words::Standing;

/// Each of these as it was: none holds a text.
macro_rules! as_it_was {
    ($($kind:ty),* $(,)?) => {
        $(impl InAscii for $kind {
            fn in_ascii(&self) -> Self {
                *self
            }
        })*
    };
}

as_it_was!(
    usize, u32, u64, f64, bool, Mark, Standing, Tone, Fill, Direction, Shape, Stroke, Branch,
    Found, Holds, Lead, Measure, Reads, Severity, SideAt, Sight,
);

/// Each of these taken apart field by field and put back with each field in ASCII.
macro_rules! field_by_field {
    ($($kind:ident { $($field:ident),* $(,)? }),* $(,)?) => {
        $(impl InAscii for $kind {
            fn in_ascii(&self) -> Self {
                let $kind { $($field),* } = self;
                $kind { $($field: $field.in_ascii()),* }
            }
        })*
    };
}

field_by_field! {
    AtAGlance { facts, acts },
    Attention { subject, asks, answer },
    Binding { keys, does },
    Card { question, edge, about, body, legend },
    Check { found, subject, said, so, fix, brief, details },
    Checklist { checks, aligned, folded, pointed, close_under_its_fold, on_their_rows },
    ChoiceList { choices, filter, legend },
    ChoiceRow { name, said, cost, key },
    Choices { rows, at },
    Code { at, within, what, lines, whole, rest, against },
    Column { title, holds },
    Concern { fact, so, fix },
    Count { standing, runs },
    Decision { chosen, options, otherwise },
    DecisionOption { id, label, tradeoff, asks },
    Desk { title, edge, gist, asked, rail, pane, foot },
    DeskFoot { question, row, legend },
    Diagram { source, chart },
    Diffstat { files, more },
    Evidence { tail, whole, kept },
    Feed { rows, before, after },
    FeedRow { subject, line },
    Field { label, mark, value, reads },
    Fields { rows },
    FileRow { path, added, removed },
    FindingList { rows, pointed },
    FindingRow { severity_mark, severity, title, answer },
    Fix { said, paste },
    Flowchart { direction, nodes, links, groups },
    Form { asked, hint, rows, focus, foot },
    FormFoot { heading, strokes, said },
    FormRow { label, value, description, refusal },
    Gist { said, on_one_row },
    Group { node, members },
    Headline { mark, subject, said, facts, edge, edge_said_elsewhere },
    Horizon { tone },
    KeyList { said, keys },
    Labelled { rows },
    LabelledRow { label, value },
    Link { from, to, label, stroke },
    Marked { mark, items },
    Marks { lines },
    Meter { count, bars, times },
    MoreFiles { files, added, removed },
    Next { steps },
    Node { id, label, shape },
    NodeRow { mark, word, id, branch, returns, note, took, took_by_person, spent },
    NodeTable { rows },
    Noted { mark, said },
    Overlay { title, doc, legend, sight, confirms },
    Pane { title, edge, place, doc, sight },
    PathNote { path, reason },
    Paths { heading, paths },
    Problem { severity, said, at, help },
    RailGroup { title, rows },
    Refusal { said, detail },
    Region { title, edge, doc, abridged, sight, measure, lists_runs },
    Ribbon { stretches, events },
    RoundRow { asked, note, stands },
    Row { mark, cells },
    RunLine { standing, handle, workflow, mode, furrow, said, edge, pointed, marked },
    Runs { rows },
    Section { lead, title, blocks },
    Side { region, at, focused, instead },
    Span { text, tone, link, fill },
    Stretch { name, standing },
    Survey { said, lists, foot },
    SurveyList { heading, rows },
    SurveyRow { mark, label, value, pointed, typed },
    Table { columns, rows },
    Time { figure, label, by_person },
    Title { name, facts },
    TypedAnswer { asked, typed, placeholder, hint },
    TypedInPlace { label, typed, cells, hint, focused },
    TypedRow { prompt, before, after },
    WordChoice { words, chosen, focused },
}

/// Each of these, a struct of one field, put back with that field in ASCII.
macro_rules! of_one_field {
    ($($kind:ident),* $(,)?) => {
        $(impl InAscii for $kind {
            fn in_ascii(&self) -> Self {
                let $kind(field) = self;
                $kind(field.in_ascii())
            }
        })*
    };
}

of_one_field!(Fold, Furrow, Legend, Note, Prose, Said, SameAtAShell);

impl InAscii for Wordmark {
    fn in_ascii(&self) -> Self {
        Wordmark
    }
}

/// A cell, its id marked again on its text in ASCII: the text before the id, the id and the
/// text after it each take their form, and the id stands where its form does. An id that marks
/// no part of the text marks none.
impl InAscii for Cell {
    fn in_ascii(&self) -> Self {
        let Cell { text, tone, id } = self;
        let parts = id.as_ref().and_then(|id| {
            let parts = [
                text.get(..id.start)?,
                text.get(id.clone())?,
                text.get(id.end..)?,
            ];
            Some(parts.map(|part| part.to_string().in_ascii()))
        });
        let Some([before, marked, after]) = parts else {
            return Cell {
                text: text.in_ascii(),
                tone: *tone,
                id: None,
            };
        };
        Cell {
            id: Some(before.len()..before.len() + marked.len()),
            text: format!("{before}{marked}{after}"),
            tone: *tone,
        }
    }
}

/// Each block in ASCII, as its kind is.
macro_rules! block_by_block {
    ($($variant:ident),* $(,)?) => {
        impl InAscii for Block {
            fn in_ascii(&self) -> Self {
                match self {
                    $(Block::$variant(block) => Block::$variant(block.in_ascii()),)*
                }
            }
        }
    };
}

block_by_block!(
    Headline,
    Furrow,
    Ribbon,
    Gist,
    Fields,
    Labelled,
    Table,
    Nodes,
    Runs,
    AtAGlance,
    Evidence,
    Said,
    Paths,
    Diffstat,
    Decision,
    Card,
    Desk,
    TypedAnswer,
    ChoiceList,
    WordChoice,
    Form,
    Survey,
    Checklist,
    Findings,
    Problem,
    Refusal,
    Next,
    SameAtAShell,
    Legend,
    KeyList,
    Attention,
    Noted,
    Title,
    Heading,
    Section,
    Fold,
    Prose,
    Note,
    Markdown,
    Marked,
    Concern,
    Diagram,
    Code,
    Lines,
    Rows,
    Feed,
    Meter,
    Wordmark,
    Horizon,
);

impl InAscii for Answer {
    fn in_ascii(&self) -> Self {
        match self {
            Answer::Command(command) => Answer::Command(command.in_ascii()),
            Answer::Key(binding) => Answer::Key(binding.in_ascii()),
            Answer::Elsewhere => Answer::Elsewhere,
            Answer::Unkeyed => Answer::Unkeyed,
        }
    }
}

impl InAscii for Keys {
    fn in_ascii(&self) -> Self {
        match self {
            Keys::UpDown => Keys::UpDown,
            Keys::LeftRight => Keys::LeftRight,
            Keys::Typed(keys) => Keys::Typed(keys.in_ascii()),
            Keys::Typing => Keys::Typing,
        }
    }
}

impl InAscii for Tally {
    fn in_ascii(&self) -> Self {
        match self {
            Tally::Uncounted => Tally::Uncounted,
            Tally::Counted(counts) => Tally::Counted(counts.in_ascii()),
        }
    }
}

impl InAscii for Edge {
    fn in_ascii(&self) -> Self {
        match self {
            Edge::Age(age) => Edge::Age(age.in_ascii()),
            Edge::Command(command) => Edge::Command(command.in_ascii()),
        }
    }
}

/// A file evidence names: what is shown of it in ASCII, and the path it links to as it is,
/// since a link is followed, never read.
impl InAscii for Whole {
    fn in_ascii(&self) -> Self {
        match self {
            Whole::File { shown, path } => Whole::File {
                shown: shown.in_ascii(),
                path: path.clone(),
            },
            Whole::Command(command) => Whole::Command(command.in_ascii()),
        }
    }
}

impl InAscii for Chosen {
    fn in_ascii(&self) -> Self {
        match self {
            Chosen::Here { commands } => Chosen::Here {
                commands: commands.in_ascii(),
            },
            Chosen::OnTheForge { forge, elsewhere } => Chosen::OnTheForge {
                forge: forge.in_ascii(),
                elsewhere: elsewhere.in_ascii(),
            },
        }
    }
}

impl InAscii for Against {
    fn in_ascii(&self) -> Self {
        match self {
            Against::New => Against::New,
            Against::Unchanged => Against::Unchanged,
            Against::Modified(marks) => Against::Modified(marks.in_ascii()),
            Against::Uncompared => Against::Uncompared,
        }
    }
}

impl InAscii for Part {
    fn in_ascii(&self) -> Self {
        match self {
            Part::Kept(words) => Part::Kept(words.in_ascii()),
            Part::Added(words) => Part::Added(words.in_ascii()),
            Part::Removed(words) => Part::Removed(words.in_ascii()),
        }
    }
}

impl InAscii for CardBody {
    fn in_ascii(&self) -> Self {
        match self {
            CardBody::Choosing { choices, then } => CardBody::Choosing {
                choices: choices.in_ascii(),
                then: then.in_ascii(),
            },
            CardBody::Saying {
                choices,
                asks,
                typed,
                refused,
            } => CardBody::Saying {
                choices: choices.in_ascii(),
                asks: asks.in_ascii(),
                typed: typed.in_ascii(),
                refused: refused.in_ascii(),
            },
            CardBody::Round(rows) => CardBody::Round(rows.in_ascii()),
        }
    }
}

impl InAscii for RoundStanding {
    fn in_ascii(&self) -> Self {
        match self {
            RoundStanding::Answered(answer) => RoundStanding::Answered(answer.in_ascii()),
            RoundStanding::Typing { typed, refused } => RoundStanding::Typing {
                typed: typed.in_ascii(),
                refused: refused.in_ascii(),
            },
            RoundStanding::Picking {
                choices,
                filter,
                refused,
            } => RoundStanding::Picking {
                choices: choices.in_ascii(),
                filter: filter.in_ascii(),
                refused: refused.in_ascii(),
            },
            RoundStanding::Ahead => RoundStanding::Ahead,
        }
    }
}

impl InAscii for RailRow {
    fn in_ascii(&self) -> Self {
        match self {
            RailRow::Stop {
                lead,
                name,
                pointed,
            } => RailRow::Stop {
                lead: lead.in_ascii(),
                name: name.in_ascii(),
                pointed: *pointed,
            },
            RailRow::Tally { what, count, key } => RailRow::Tally {
                what: what.in_ascii(),
                count: *count,
                key: key.in_ascii(),
            },
            RailRow::Weighs(weighs) => RailRow::Weighs(weighs.in_ascii()),
        }
    }
}

impl InAscii for FootRow {
    fn in_ascii(&self) -> Self {
        match self {
            FootRow::Options { named, more } => FootRow::Options {
                named: named.in_ascii(),
                more: *more,
            },
            FootRow::Confirming(chosen) => FootRow::Confirming(chosen.in_ascii()),
            FootRow::Saying {
                chosen,
                asks,
                typed,
                refused,
            } => FootRow::Saying {
                chosen: chosen.in_ascii(),
                asks: asks.in_ascii(),
                typed: typed.in_ascii(),
                refused: refused.in_ascii(),
            },
        }
    }
}

impl InAscii for FormValue {
    fn in_ascii(&self) -> Self {
        match self {
            FormValue::Typed { typed, placeholder } => FormValue::Typed {
                typed: typed.in_ascii(),
                placeholder: placeholder.in_ascii(),
            },
            FormValue::Listed(choices) => FormValue::Listed(choices.in_ascii()),
            FormValue::Words(words) => FormValue::Words(words.in_ascii()),
        }
    }
}

impl InAscii for SurveyValue {
    fn in_ascii(&self) -> Self {
        match self {
            SurveyValue::Found(found) => SurveyValue::Found(found.in_ascii()),
            SurveyValue::Composed(line) => SurveyValue::Composed(line.in_ascii()),
            SurveyValue::Chosen(words) => SurveyValue::Chosen(words.in_ascii()),
        }
    }
}

impl Screen {
    /// This screen with every text it holds in ASCII, as a full-screen terminal drawn in the
    /// ASCII glyphs lays it out: each character takes its ASCII form before anything is
    /// measured (`·` `;`, `—` `--`, `ñ` `n`, and `?` for one with no form), so every row a
    /// terminal draws of it ends at its width where no UTF-8 locale is named. Markdown, HTML
    /// and a printed page draw the screen as it is, every character kept.
    pub fn in_ascii(&self) -> Screen {
        let Screen {
            trail,
            tally,
            about,
            main,
            side,
            meter,
            attention,
            noted,
            legend,
            anywhere,
            typed,
            overlay,
            title,
        } = self;
        Screen {
            trail: trail.in_ascii(),
            tally: tally.in_ascii(),
            about: about.in_ascii(),
            main: main.in_ascii(),
            side: side.in_ascii(),
            meter: meter.in_ascii(),
            attention: attention.in_ascii(),
            noted: noted.in_ascii(),
            legend: legend.in_ascii(),
            anywhere: anywhere.in_ascii(),
            typed: *typed,
            overlay: overlay.in_ascii(),
            title: title.in_ascii(),
        }
    }
}

#[cfg(test)]
mod tests;
