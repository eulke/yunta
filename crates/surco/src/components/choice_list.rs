//! The list a person picks one value from, narrowed by what they type.

use super::{Choices, Legend};

/// A list a person picks from: the rows what was typed leaves, with the one pointed
/// at, what was typed to narrow it, and the keys it answers to. Not a
/// [`Card`](super::Card), whose options a decision offers.
pub struct ChoiceList {
    /// What the filter leaves, or `None` when it leaves nothing.
    pub choices: Option<Choices>,
    /// What was typed to narrow it, when anything was.
    pub filter: Option<String>,
    pub legend: Legend,
}

#[cfg(test)]
pub(crate) mod tests;
