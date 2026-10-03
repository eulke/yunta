//! What a surface needs to know about the stream it is drawn on.

use super::ink::Ink;
use super::{Glyphs, Width};

/// How one stream's lines are drawn: the characters they are drawn
/// with, the paint they get and the cells they may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub glyphs: Glyphs,
    pub ink: Ink,
    pub width: Width,
}

impl Look {
    /// The look of a line with nothing but its words: ASCII, unpainted,
    /// at the width a line has off a terminal — what a test compares a
    /// surface against, and what a reader with no terminal gets.
    pub fn plain() -> Self {
        Look {
            glyphs: Glyphs::Ascii,
            ink: Ink::Plain,
            width: Width::of(None, None),
        }
    }

    /// The look of a terminal a golden is rendered for.
    #[cfg(test)]
    pub fn of(environment: &yunta_testkit_core::golden::Environment) -> Self {
        Look {
            glyphs: match environment.unicode {
                true => Glyphs::Unicode,
                false => Glyphs::Ascii,
            },
            ink: match environment.color {
                true => Ink::Ansi16,
                false => Ink::Plain,
            },
            width: Width::of(Some(environment.width), None),
        }
    }
}
