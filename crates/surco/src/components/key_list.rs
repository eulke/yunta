//! The keys a person can press, listed a key a row with what each does, under the sentence
//! that says what they are: what help holds. The keys are a [`Legend`]'s, so a key is said
//! one way whether a screen's last row names it or help lists it.

use super::Legend;

/// The keys a person can press, a key a row with what each does, under a sentence that says
/// what they are. Not a [`Legend`] block, a screen's keys on one row.
#[derive(Clone)]
pub struct KeyList {
    /// What the keys are, said above them; none where nothing is.
    pub said: Option<String>,
    pub keys: Legend,
}

#[cfg(test)]
pub(crate) mod tests;
