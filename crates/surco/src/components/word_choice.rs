//! A row of words with one chosen: what a person changes with `←→` where a value is one of a
//! few that fit on a row, as a new run's mode and adapter, or a role's adapter in a setup.
//!
//! The chosen word is marked by the pointer before it, not by a fill alone, so a layer with no
//! tones still says which word is chosen; a fill on one layer would add no fact.

/// Words in one row, apart, one of them chosen: the chosen one marked by the pointer before it,
/// and strong, the rest muted. Not [`ChoiceList`](super::ChoiceList), a list a pointer moves
/// along, row after row.
pub struct WordChoice {
    /// The words, in the order they stand.
    pub words: Vec<String>,
    /// Which word is chosen, counted from the first; past the last word, the last.
    pub chosen: usize,
    /// Whether the keys act on the row: its pointer then stands in the tone of a pointer a
    /// person moves, and muted where they act elsewhere. The words read the same either way.
    pub focused: bool,
}

impl WordChoice {
    /// Where the chosen word stands among the words: `chosen`, held to the last word.
    pub(crate) fn chosen_word(&self) -> usize {
        self.chosen.min(self.words.len().saturating_sub(1))
    }
}

#[cfg(test)]
pub(crate) mod tests;
