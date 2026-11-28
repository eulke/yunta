//! A line asked of a person, and the answer typed under it.
//!
//! The words are typed in a box, the row a person types on ([`TypedRow`]). Around it the
//! answer can say two things the person reads before they type: a placeholder, an example of
//! what the line takes, which stands faint inside the box while nothing is typed and goes
//! with the first key; and a hint, what becomes of the words, which stands muted under the
//! box while they are typed. A line whose question says it all takes neither.

use super::TypedRow;
use crate::line::Line;

/// A line asked of a person and the row its answer is typed on: what a prompt draws
/// when it asks for one line. Not a [`Card`](super::Card)'s typed row, which stands
/// under the option chosen.
pub struct TypedAnswer {
    /// What is asked, as its caller says it.
    pub asked: Line,
    pub typed: TypedRow,
    /// What such an answer reads like, standing faint where the words go until the first
    /// key is typed. Not what is typed: it is never the answer.
    pub placeholder: Option<String>,
    /// What becomes of the words, standing muted under the row they are typed on, typed or
    /// not.
    pub hint: Option<String>,
}

#[cfg(test)]
pub(crate) mod tests;
