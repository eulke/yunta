//! A screen's rows that are no region's document, as a terminal draws them: where the
//! person is with how many runs stand at each word that asks for a look, the rule under
//! that row, each region's title, and the edge between the side region and the main one.
//! An engine of cells lays the rows out; every word, tone and mark in them is drawn here.

use super::{at_ends, Terminal, APART};
use crate::components::{Count, Furrow};
use crate::diagram::Stroke;
use crate::line::{Line, Tone};
use crate::width::cell_width;

impl Terminal {
    /// Where the person is, from the outermost place, the first strong and the rest
    /// plain, and the tally at the row's end, each count with its standing's mark, in its
    /// tone. A row too short for both cuts the trail and keeps the tally whole.
    pub fn trail(&self, trail: &[String], tally: &[Count]) -> Line {
        let glyphs = self.look.glyphs;
        let cells = self.look.width.cells();
        let within = format!(" {} ", glyphs.within());
        let places = trail
            .iter()
            .enumerate()
            .fold(Line::new(), |line, (at, place)| match at {
                0 => line.push(Tone::Strong, place.as_str()),
                _ => line
                    .push(Tone::Muted, within.as_str())
                    .push(Tone::Plain, place.as_str()),
            });
        let sep = format!(" {} ", glyphs.sep());
        let counted = tally
            .iter()
            .enumerate()
            .fold(Line::new(), |line, (at, count)| {
                let mark = count.standing.mark();
                let tone = Tone::of(mark);
                let line = match at {
                    0 => line,
                    _ => line.push(Tone::Muted, sep.as_str()),
                };
                line.push(tone, format!("{} {}", glyphs.mark(mark), count.said()))
            });
        if tally.is_empty() {
            return places.cut(cells, glyphs);
        }
        let counted = counted.cut(cells, glyphs);
        let room = cells.saturating_sub(cell_width(&counted.text()) + APART);
        at_ends(places.cut(room, glyphs), Some(counted), cells)
    }

    /// The rule under the trail: the furrow of no node, ground not yet plowed across the
    /// width.
    pub fn rule(&self) -> Line {
        Furrow(Vec::new())
            .lines(&self.look)
            .into_iter()
            .next()
            .unwrap_or_default()
    }

    /// The edge between the side region and the main one, on each of their rows: an upright
    /// with a cell either side, faint. Its cells are what an engine of cells leaves for it.
    pub fn edge(&self) -> Line {
        let upright = self
            .look
            .glyphs
            .joint(true, true, false, false, Stroke::Open);
        Line::new().push(Tone::Faint, format!(" {upright} "))
    }

    /// A region's title, strong where the keys act on the region and muted elsewhere, cut
    /// to the width.
    pub fn title(&self, title: &str, focused: bool) -> Line {
        let tone = match focused {
            true => Tone::Strong,
            false => Tone::Muted,
        };
        Line::new()
            .push(tone, title)
            .cut(self.look.width.cells(), self.look.glyphs)
    }
}

#[cfg(test)]
mod tests;
