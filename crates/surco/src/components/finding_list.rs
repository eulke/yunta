//! What a review found, a finding a row: the mark of its severity, or the done mark once it
//! is answered, its severity's word in a column as wide as the widest word on the list, and
//! its title. An answered finding keeps its row and its severity's word, its answer under its
//! title: a struck row has no plain form, and every layer of a stream says the same. The list
//! keeps the order its caller gives it; which findings it holds, and in what order, is the
//! caller's to say.

use crate::glyphs::Mark;

/// One finding as a list shows it: the mark and the word of its severity, its title, and
/// what answered it once something did. Not a [`Check`](super::Check), what one check of a
/// report found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindingRow {
    /// The mark its severity is drawn with while it stands open.
    pub severity_mark: Mark,
    /// The word of its severity, as the review said it.
    pub severity: String,
    pub title: String,
    /// What answered it, as said (`fixed: …`); none while it stands open.
    pub answer: Option<String>,
}

impl FindingRow {
    /// A finding of `severity`, drawn with `severity_mark` while it stands open, titled
    /// `title`.
    pub fn new(severity_mark: Mark, severity: impl Into<String>, title: impl Into<String>) -> Self {
        FindingRow {
            severity_mark,
            severity: severity.into(),
            title: title.into(),
            answer: None,
        }
    }

    /// This finding answered with `answer`.
    pub fn answered(mut self, answer: impl Into<String>) -> Self {
        self.answer = Some(answer.into());
        self
    }

    /// The mark its row leads with: the done mark once it is answered, its severity's before.
    pub fn mark(&self) -> Mark {
        match self.answer {
            Some(_) => Mark::Done,
            None => self.severity_mark,
        }
    }
}

/// The findings of a review, one a row, with the row a pointer stands on where a person or
/// the run moves along them. Not a [`Checklist`](super::Checklist), the checks of one report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindingList {
    pub rows: Vec<FindingRow>,
    /// The row the pointer stands on, counted from the first; none where nothing points.
    pub pointed: Option<usize>,
}

impl FindingList {
    /// `rows`, in their order, with no pointer.
    pub fn new(rows: Vec<FindingRow>) -> Self {
        FindingList {
            rows,
            pointed: None,
        }
    }

    /// This list with the pointer on the row at `at`.
    pub fn pointing_at(mut self, at: usize) -> Self {
        self.pointed = Some(at);
        self
    }
}

#[cfg(test)]
pub(crate) mod tests;
