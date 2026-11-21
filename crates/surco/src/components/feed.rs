//! What a pane shows of a list that only grows at its end: the rows in sight, the newest
//! last, and how many stand out of sight above and under them.
//!
//! A feed of more than one thing's moments, as a run's whole chronicle, says what each row is
//! about before it, in a column as wide as the widest subject a surface shows and two cells:
//! which rows stand in sight is the surface's to know, so the column is laid out there, and
//! the words of every row start at one column whichever subjects stand in sight.

use crate::line::Line;

/// The rows in sight of a list that only grows at its end, the newest last, with how many
/// rows stand above them and how many under them: what happens at a node, a row a moment, as
/// a pane and a printed page show it. A surface says each count where it is not zero, and
/// cuts a row at its edge rather than fold it, so a row stays one moment. Not the CLI's
/// `Beats`, a queue of what a run's surface draws.
pub struct Feed {
    /// The rows in sight, the oldest first, each as its caller composed it.
    pub rows: Vec<Line>,
    /// What each row is about, one a row in the rows' order, where the feed lists the moments
    /// of more than one thing: said before its row, in the row's opening tone. Empty where
    /// every row is one thing's, as a node's feed under its pane's title.
    pub subjects: Vec<String>,
    /// How many rows stand above the rows in sight.
    pub before: usize,
    /// How many rows stand under the rows in sight.
    pub after: usize,
}

#[cfg(test)]
pub(crate) mod tests;
