//! What a command printed, quoted: the end of it, and where the rest is.

use super::Block;
use crate::render::ink::{Line, Tone};
use crate::render::{cell_width, truncate, Look, INDENT};

/// The lines evidence quotes at most: the end of what a command printed,
/// which is where a compiler and a test runner say what went wrong. More
/// buries the verdict under the output it came from; the rest is one
/// command or one path away.
const QUOTED: usize = 6;

/// The end of what a command printed, and where the whole of it is.
pub(crate) struct Evidence {
    pub(crate) tail: Vec<String>,
    /// Where the whole output is, as a reader reads a path.
    pub(crate) whole: Option<String>,
}

impl Block for Evidence {
    /// The last [`QUOTED`] lines, each hanging from the gutter and cut to
    /// the line rather than wrapped — what a command printed keeps its
    /// shape — then how much came before them and where it is.
    fn lines(&self, look: &Look) -> Vec<Line> {
        let gutter = format!("{INDENT}{} ", look.glyphs.gutter());
        let room = look.width.cells().saturating_sub(cell_width(&gutter));
        let skipped = self.tail.len().saturating_sub(QUOTED);
        let mut lines: Vec<Line> = self
            .tail
            .iter()
            .skip(skipped)
            .map(|said| {
                Line::new()
                    .push(Tone::Muted, gutter.as_str())
                    .plain(truncate(said, room, look.glyphs).trim_end())
            })
            .collect();
        let earlier =
            (skipped > 0).then(|| format!("{} above", yunta_core::text::counted(skipped, "line")));
        let whole = self
            .whole
            .as_ref()
            .map(|whole| format!("whole output: {whole}"));
        let rest: Vec<String> = earlier.into_iter().chain(whole).collect();
        // One line when it fits, and each fact on its own when it does
        // not: a path cut to fit is a path nobody can open.
        let joined = rest.join(&format!(" {} ", look.glyphs.sep()));
        let room = look.width.cells().saturating_sub(cell_width(INDENT));
        let said = match cell_width(&joined) <= room {
            true => vec![joined],
            false => rest,
        };
        lines.extend(
            said.into_iter()
                .filter(|part| !part.is_empty())
                .map(|part| Line::new().plain(INDENT).push(Tone::Muted, part)),
        );
        lines
    }
}
