//! The field in perspective: furrows running to a horizon, a picture under what a screen
//! says, which itself says nothing.
//!
//! It is drawing alone: no word, figure or mark stands in it, so a reader who never sees it
//! loses no fact. A terminal draws it in braille cells, the only cells fine enough to draw a
//! line at a slant; ASCII has none, so a terminal limited to it leaves the field's rows
//! blank rather than draw furrows in characters that read as text. A surface that draws no
//! picture, Markdown or a page, leaves it out.

use crate::line::Tone;

/// Furrows running to a horizon: a field seen from inside it, from its foot to where its
/// furrows meet. It fills the rows a region leaves it, and says nothing. Not a
/// [`Furrow`](super::Furrow), a run's nodes as strokes, each saying where one stands.
pub struct Horizon {
    /// The tone its furrows are drawn in: [`Tone::Done`] under a receipt, the harvested
    /// field; [`Tone::Faint`] under a page that only stands on it.
    pub tone: Tone,
}
