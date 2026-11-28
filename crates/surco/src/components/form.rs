//! What a person sets before an act starts, a row each, the keys on one row at a time: a line
//! typed in a box, a list a pointer moves along, or a row of words with one chosen. A new run
//! is one: its request, its workflow, its other inputs, its mode and its adapter.
//!
//! What the form asks stands over its first row and what becomes of what is set under that row,
//! each said once for the whole form; every other row is labelled over what is set on it and
//! described under it. What refused the act over what a row holds stands under that row, opened
//! by the mark of a failure: a new run's input its start was refused for. What the rows come to stands at the form's foot, under them all, a new
//! run's `what runs`: where each step it would take stands, as strokes, and what it says of
//! them; it gives way before any row does.

use super::{Choices, Stretch, TypedRow, WordChoice};
use crate::line::Line;

/// What a person sets before an act starts, a row each, one row in focus: what a new run asks
/// before it starts. Not a [`Card`](super::Card)'s round, whose questions are answered one after
/// another, nor a [`ChoiceList`](super::ChoiceList), a list narrowed by typing.
pub struct Form {
    /// What the form asks, over its first row: said once for the whole form, as the first
    /// request's question stands over a new run's request.
    pub asked: Option<Line>,
    /// What becomes of what is set, under the first row: said once for the whole form, as a new
    /// run's hint says what the request becomes and how the workflow asks.
    pub hint: Option<String>,
    /// The rows, in the order they stand.
    pub rows: Vec<FormRow>,
    /// The row the keys act on, counted from the first; none where the keys act on no row.
    pub focus: Option<usize>,
    /// What the rows come to, under them: a new run's `what runs`.
    pub foot: Option<FormFoot>,
}

/// What a [`Form`]'s rows come to, under them: its heading, then, a step in, where each step it
/// would take stands, a stroke each as a list of runs draws a run's furrow on its row, and what
/// it says of them, a line each. A new run's `what runs`: a stroke for each node its workflow
/// declares, the nodes its mode leaves out drawn as left out, then the names of those it runs
/// and where it stops for the person. A surface that draws no strokes says each step by its
/// name and where it stands, as it says a ribbon. Not a run's furrow under its headline, which
/// spans the row.
pub struct FormFoot {
    pub heading: Line,
    /// Each step, in order, by its name and where it stands; none draws no strokes.
    pub strokes: Vec<Stretch>,
    /// What it says under the strokes, each line going on under itself where it is wider than
    /// its row.
    pub said: Vec<Line>,
}

/// One row of a [`Form`]: what it sets, over it; what is set on it; what that is, under it;
/// and what refused the act over it, last.
pub struct FormRow {
    /// What the row sets, over it: none on the first row, which the form's question asks.
    pub label: Option<Line>,
    pub value: FormValue,
    /// What the row's value is, under it: an input's description.
    pub description: Option<String>,
    /// Why the act the form asks was refused over what the row holds, in the refusal's own
    /// words, under its description: none where nothing refused it.
    pub refusal: Option<String>,
}

/// What is set on a row of a [`Form`].
pub enum FormValue {
    /// Words typed in a box, a placeholder faint in it while nothing is typed: the cursor
    /// stands in it while the row is in focus.
    Typed {
        typed: TypedRow,
        placeholder: Option<String>,
    },
    /// One of a list's rows, the pointer on the one chosen.
    Listed(Choices),
    /// One of a few words on one row.
    Words(WordChoice),
}

#[cfg(test)]
pub(crate) mod tests;
