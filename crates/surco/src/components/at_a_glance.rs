//! A run seen at a glance beside a list of runs: what a person reads of the run the list points
//! at before they open it, and what they can do about it from there.
//!
//! Its facts are words a person reads, each beside its label, its value wrapped under its own
//! first cell where it passes its row, never inside a figure. Its acts are keys, each with what
//! it does, under the heading that says what they are: the keys of the list it stands beside,
//! so a key is said one way whether the glance names it or a legend does. A fact with nothing
//! to say is no row, and a glance with no fact and no act draws nothing.

use super::Binding;

/// A run seen at a glance beside a list: its facts, each label in one column and its value
/// wrapped under itself, then the acts it offers under their heading, a key a row, the keys in
/// one column. Not [`Fields`](super::Fields), a page's facts, each read as words, as written or
/// as a command; not [`Depth::Glance`](crate::doc::Depth::Glance), how much a page says.
pub struct AtAGlance {
    /// Each fact's label and what it says, in the order they are read; one that says nothing
    /// is no row.
    pub facts: Vec<(&'static str, String)>,
    /// What the acts are, said above them, and each act's key with what it does; none where
    /// the run offers nothing here.
    pub acts: Option<(String, Vec<Binding>)>,
}

impl AtAGlance {
    /// The facts that say something, in their order.
    pub(crate) fn said(&self) -> impl Iterator<Item = &(&'static str, String)> {
        self.facts
            .iter()
            .filter(|(_, value)| !value.trim().is_empty())
    }
}

#[cfg(test)]
pub(crate) mod tests;
