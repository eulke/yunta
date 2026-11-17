//! What a pane shows of a list that only grows at its end: the rows in sight, the newest
//! last, and how many stand out of sight above and under them.

use crate::line::Line;

/// The rows in sight of a list that only grows at its end, the newest last, with how many
/// rows stand above them and how many under them: what happens at a node, a row a moment, as
/// a pane and a printed page show it. A surface says each count where it is not zero, and
/// cuts a row at its edge rather than fold it, so a row stays one moment. Not the CLI's
/// `Beats`, a queue of what a run's surface draws.
pub struct Feed {
    /// The rows in sight, the oldest first, each as its caller composed it.
    pub rows: Vec<Line>,
    /// How many rows stand above the rows in sight.
    pub before: usize,
    /// How many rows stand under the rows in sight.
    pub after: usize,
}

#[cfg(test)]
pub(crate) mod tests;
