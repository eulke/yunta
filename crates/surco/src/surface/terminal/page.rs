//! The furrow and the gist, as they hang from a terminal page's headline.

use crate::components::{Furrow, Gist};
use crate::glyphs::{Glyphs, Plow};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::paint::Ink;
use crate::width::{cell_width, wrap_unbroken, INDENT};
use crate::words::Standing;

impl Furrow {
    /// One stroke for each node, as wide as the line shares out, a gap
    /// between two: plowed where the node has stood somewhere, in the tone
    /// of its word; dashed where it never ran; dotted where the run leaves
    /// it out. With neither color nor Unicode it is a rule, and the words
    /// beside it say how far the run has come.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let width = look.width.cells();
        let nodes = self.0.len();
        if nodes == 0 {
            return Vec::new();
        }
        if look.glyphs == Glyphs::Ascii && look.ink == Ink::Plain {
            return vec![
                Line::new().push(Tone::Muted, look.glyphs.rule().to_string().repeat(width))
            ];
        }
        let gaps = match width >= nodes * 2 {
            true => nodes - 1,
            false => 0,
        };
        let cells = width.saturating_sub(gaps);
        let mut line = Line::new();
        for (at, standing) in self.0.iter().enumerate() {
            let share = |upto: usize| (cells * upto + nodes / 2) / nodes;
            let takes = share(at + 1) - share(at);
            let (plow, tone) = match standing {
                Standing::Skipped => (Plow::Skipped, Tone::Muted),
                Standing::NeverRan => (Plow::Ahead, Tone::Muted),
                _ => (Plow::Plowed, Tone::of(standing.mark())),
            };
            line = line.push(tone, look.glyphs.furrow(plow).to_string().repeat(takes));
            if gaps > 0 && at + 1 < nodes {
                line = line.plain(" ");
            }
        }
        vec![line]
    }
}

impl Gist {
    /// The sentence wrapped one step in, whole.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let room = look.width.cells().saturating_sub(cell_width(INDENT));
        wrap_unbroken(&self.0, room)
            .into_iter()
            .map(|part| Line::new().plain(INDENT).plain(part))
            .collect()
    }
}
