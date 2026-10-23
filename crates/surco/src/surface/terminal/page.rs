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
    /// beside it say how far the run has come. A furrow with no node is
    /// ground not yet plowed: the dim line a page about no one run stands
    /// under.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let width = look.width.cells();
        let plain = look.glyphs == Glyphs::Ascii && look.ink == Ink::Plain;
        if plain || self.0.is_empty() {
            let stroke = match plain {
                true => look.glyphs.rule(),
                false => look.glyphs.furrow(Plow::Ahead),
            };
            return vec![Line::new().push(Tone::Muted, stroke.to_string().repeat(width))];
        }
        let apart = width >= self.0.len() * 2;
        vec![strokes(&self.0, width, apart, look)]
    }
}

/// Where each of `nodes` stands, as strokes sharing `cells` between them:
/// plowed in the tone of the node's word, dashed ahead, dotted left out —
/// a space between two strokes when `apart`.
pub(super) fn strokes(nodes: &[Standing], cells: usize, apart: bool, look: &Look) -> Line {
    let count = nodes.len();
    let gaps = match apart {
        true => count.saturating_sub(1),
        false => 0,
    };
    let cells = cells.saturating_sub(gaps);
    let mut line = Line::new();
    for (at, standing) in nodes.iter().enumerate() {
        let share = |upto: usize| (cells * upto + count / 2) / count;
        let takes = share(at + 1) - share(at);
        let (plow, tone) = match standing {
            Standing::Skipped => (Plow::Skipped, Tone::Muted),
            Standing::NeverRan => (Plow::Ahead, Tone::Muted),
            _ => (Plow::Plowed, Tone::of(standing.mark())),
        };
        line = line.push(tone, look.glyphs.furrow(plow).to_string().repeat(takes));
        if gaps > 0 && at + 1 < count {
            line = line.plain(" ");
        }
    }
    line
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
