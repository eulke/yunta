//! A line asked of a person, and the answer typed under it.

use super::TypedRow;
use crate::line::Line;

/// A line asked of a person and the row its answer is typed on: what a prompt draws
/// when it asks for one line. Not a [`Card`](super::Card)'s typed row, which stands
/// under the option chosen.
pub struct TypedAnswer {
    /// What is asked, as its caller says it.
    pub asked: Line,
    pub typed: TypedRow,
}

#[cfg(test)]
pub(crate) mod tests;
