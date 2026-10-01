//! What a surface needs to know about the stream it is drawn on.

use super::{Glyphs, Width};

/// How one stream's lines are drawn: the characters they are drawn
/// with and the cells they may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Look {
    pub(crate) glyphs: Glyphs,
    pub(crate) width: Width,
}

impl Look {
    /// The look stdout gets in this process.
    pub(crate) fn stdout() -> Self {
        Look {
            glyphs: Glyphs::from_env(),
            width: Width::stdout(),
        }
    }

    /// The look of a line read off a terminal: for a test, which
    /// compares what a surface drew against text it wrote down.
    #[cfg(test)]
    pub(crate) fn plain() -> Self {
        Look {
            glyphs: Glyphs::Ascii,
            width: Width::of(None, None),
        }
    }
}
