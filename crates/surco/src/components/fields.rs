//! Labelled facts, one to a row, the label in a column of its own.

use crate::glyphs::Mark;
use crate::line::Tone;

/// Labelled rows. A fact with nothing to say is not a row: a label over
/// an empty value tells a reader to look for something that is not
/// there. A row whose label is the one above it goes on saying what that
/// row says, under it.
#[derive(Default)]
pub struct Fields {
    pub(crate) rows: Vec<Field>,
}

/// One fact: its label, the mark it opens with when it is one of several
/// kinds, what it says, and what that is to a reader.
pub struct Field {
    pub label: &'static str,
    pub mark: Option<Mark>,
    pub value: String,
    pub tone: Tone,
}

impl Fields {
    pub fn new() -> Self {
        Fields::default()
    }

    /// These fields with `value` under `label`, when it says anything.
    pub fn push_if(self, label: &'static str, value: impl Into<String>) -> Self {
        self.push(label, None, value.into(), Tone::Plain)
    }

    /// `command` beside `label`: a row a reader copies.
    pub fn push_command(self, label: &'static str, command: impl Into<String>) -> Self {
        self.push(label, None, command.into(), Tone::Command)
    }

    /// `value` under `label`, opening with `mark`: one of several kinds of
    /// the same fact, each on its row.
    pub fn push_marked(self, label: &'static str, mark: Mark, value: impl Into<String>) -> Self {
        self.push(label, Some(mark), value.into(), Tone::Plain)
    }

    fn push(mut self, label: &'static str, mark: Option<Mark>, value: String, tone: Tone) -> Self {
        if !value.trim().is_empty() {
            self.rows.push(Field {
                label,
                mark,
                value,
                tone,
            });
        }
        self
    }

    /// Each row, in order.
    pub fn rows(&self) -> impl Iterator<Item = &Field> {
        self.rows.iter()
    }

    /// Whether any row says anything.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}
