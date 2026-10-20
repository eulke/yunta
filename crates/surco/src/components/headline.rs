//! The line a surface opens with: what it is about, and the word for
//! where that stands.

use crate::glyphs::Mark;

/// What a surface is about — `run 7E5PH4` — and where it stands, marked.
pub struct Headline {
    pub subject: String,
    pub mark: Mark,
    pub said: String,
}
