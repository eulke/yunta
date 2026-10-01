//! What a surface needs to know about the stream it is drawn on.

use super::ink::Ink;
use super::{Glyphs, Width};

/// How one stream's lines are drawn: the characters they are drawn
/// with, the paint they get and the cells they may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Look {
    pub(crate) glyphs: Glyphs,
    pub(crate) ink: Ink,
    pub(crate) width: Width,
}

impl Look {
    /// The look stdout gets in this process.
    pub(crate) fn stdout() -> Self {
        Look {
            glyphs: Glyphs::from_env(),
            ink: Ink::stdout(),
            width: Width::stdout(),
        }
    }

    /// The look of a line read off a terminal: for a test, which
    /// compares what a surface drew against text it wrote down.
    #[cfg(test)]
    pub(crate) fn plain() -> Self {
        Look {
            glyphs: Glyphs::Ascii,
            ink: Ink::Plain,
            width: Width::of(None, None),
        }
    }

    /// The look of a terminal a golden is rendered for.
    #[cfg(test)]
    pub(crate) fn of(environment: &yunta_testkit::Environment) -> Self {
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
