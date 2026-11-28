//! What was found and what an act will do with it, read as rows under headings and changed in
//! place before the act: a setup's facts and roles, over the row that writes them.
//!
//! Each row is a mark, a label in one column across every list, and what stands for it: words
//! found, a row its caller composed (a role's candidates in their columns), or, while the row is
//! changed, a row of words with one chosen. A row being changed can open a line typed in place
//! under it, in a box whose label stands in its top edge, with what becomes of the words under
//! the box. The pointer stands in a row's margin, so every row stands where it would without it.
//! The act stands at the foot, a row of its own, and never leaves sight; what is said over the
//! rows, a sentence, stands first.
//!
//! A surface that cannot hold it all gives way in an order of its own (the terminal's
//! `survey`): the blank row under the sentence, then what stands above it whole, then a window
//! over its rows that keeps the row pointed at and the foot in sight and counts the rows it
//! hides.

use super::{TypedRow, WordChoice};
use crate::doc::Block;
use crate::glyphs::{Glyphs, Mark};
use crate::line::{Line, Tone};

/// What was found and what an act will do with it, a row each under headings, the pointer on
/// one, and the act at the foot: what a setup shows before it writes. Not a [`Form`], whose rows
/// are labelled over what is set on them and whose act has no row, nor a [`Checklist`], which
/// says what checks found and changes nothing.
///
/// [`Form`]: super::Form
/// [`Checklist`]: super::Checklist
pub struct Survey {
    /// What it says over its rows: a sentence, which a pane folds at its edge and a column
    /// lets stand across it (`Width::across`).
    pub said: Option<Line>,
    /// Each list of rows under its heading, in the order they stand.
    pub lists: Vec<SurveyList>,
    /// The act on all of it, a row of its own at the foot, composed by its caller with its
    /// pointer where it stands there: it never gives way.
    pub foot: Line,
}

/// One list of a [`Survey`]: its heading and its rows.
pub struct SurveyList {
    pub heading: String,
    pub rows: Vec<SurveyRow>,
}

/// One row of a [`Survey`]: its mark, its label, what stands for it, whether the pointer stands
/// on it, and the line typed in place under it while it is changed.
pub struct SurveyRow {
    pub mark: Mark,
    pub label: String,
    pub value: SurveyValue,
    pub pointed: bool,
    pub typed: Option<TypedInPlace>,
}

/// What stands for a row of a [`Survey`], after its label.
pub enum SurveyValue {
    /// Words found, going on under themselves where they pass the row.
    Found(String),
    /// A row its caller composed, cut at the row's edge: a role's candidates in their columns.
    Composed(Line),
    /// A row of words with one chosen, which the arrows change, the chosen word where the
    /// rows' values stand: a role's adapter, changed in place.
    Chosen(WordChoice),
}

/// A line typed in place under a row of a [`Survey`] while it is changed: in a box as wide as
/// `cells`, where the row's value stands, its label in the box's top edge, and what becomes of
/// the words muted under it. Not a [`TypedAnswer`](super::TypedAnswer), whose question stands
/// over its box.
pub struct TypedInPlace {
    /// What the line takes, in the box's top edge: `model`.
    pub label: String,
    pub typed: TypedRow,
    /// The cells the box takes, at most what the row leaves it.
    pub cells: usize,
    /// What becomes of the words, under the box.
    pub hint: Option<String>,
    /// Whether the keys type in it: the cursor stands in it only then.
    pub focused: bool,
}

impl Survey {
    /// Whether a line is typed in it: a row's line in focus, where the cursor stands.
    pub fn typing(&self) -> bool {
        self.lists
            .iter()
            .flat_map(|list| &list.rows)
            .any(|row| row.typed.as_ref().is_some_and(|typed| typed.focused))
    }

    /// What it says as blocks every surface draws, for one that lays no rows out of its own
    /// (a file): the sentence, each list under its heading, a row a line with the pointer
    /// before the row pointed at and the line typed under the row it is typed in, then the
    /// act's row.
    pub(crate) fn blocks(&self, glyphs: Glyphs) -> Vec<Block> {
        let said = self
            .said
            .iter()
            .map(|said| Block::Lines(vec![said.clone()]));
        let lists = self.lists.iter().flat_map(|list| {
            let rows = list.rows.iter().flat_map(|row| row.said(glyphs)).collect();
            [Block::Heading(list.heading.clone()), Block::Rows(rows)]
        });
        said.chain(lists)
            .chain([Block::Rows(vec![self.foot.clone()])])
            .collect()
    }
}

impl SurveyRow {
    /// The row as a file writes it, in `glyphs`: the pointer where it stands here, the mark,
    /// the label strong, what stands for it; then the line typed in place, after its label,
    /// and what becomes of it.
    fn said(&self, glyphs: Glyphs) -> Vec<Line> {
        let pointer = match self.pointed {
            true => format!("{} ", glyphs.pointer()),
            false => String::new(),
        };
        let value = match &self.value {
            SurveyValue::Found(found) => Line::new().plain(found.as_str()),
            SurveyValue::Composed(line) => line.clone(),
            SurveyValue::Chosen(choice) => {
                let words: Vec<String> = (choice.words.iter().enumerate())
                    .map(|(at, word)| match at == choice.chosen_word() {
                        true => format!("{} {word}", glyphs.pointer()),
                        false => word.clone(),
                    })
                    .collect();
                Line::new().plain(words.join("  "))
            }
        };
        let row = Line::new()
            .plain(pointer)
            .push(Tone::of(self.mark), glyphs.mark(self.mark).to_string())
            .plain(" ")
            .push(Tone::Strong, self.label.as_str())
            .plain(" ")
            .then(value);
        let typed = self.typed.iter().flat_map(|typed| {
            let (before, after) = (&typed.typed.before, &typed.typed.after);
            let line = Line::new()
                .push(Tone::Muted, format!("{}: ", typed.label))
                .push(Tone::Strong, format!("{before}{after}"));
            let hint = typed
                .hint
                .iter()
                .map(|hint| Line::new().push(Tone::Muted, hint));
            std::iter::once(line).chain(hint)
        });
        std::iter::once(row).chain(typed).collect()
    }
}

#[cfg(test)]
pub(crate) mod tests;
