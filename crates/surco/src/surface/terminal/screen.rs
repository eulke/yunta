//! A screen's rows that are no region's document, as a terminal draws them: where the
//! person is with how many runs stand at each word that asks for a look, the rule under
//! that row, what the screen is about across it, each region's title with its edge at the
//! far end, the edge between the side region and the main one, and the note that stands in
//! the attention row's place. An engine of cells lays the rows out; every word, tone and
//! mark in them is drawn here.
//!
//! What a screen is about is a band across it, not a page: its blocks stand one under
//! another with no blank row between them, and in the rows the engine leaves it a ribbon's
//! names give way first, its strokes standing alone, before the band is cut from its last
//! row. Not [`Terminal::lines`], which lays a document out as a page with a blank row
//! between two blocks.

use super::{at_ends, Terminal, APART};
use crate::components::{Count, Furrow, Noted, Region, Screen, Tally};
use crate::diagram::Stroke;
use crate::doc::{Block, Doc};
use crate::glyphs::{Glyphs, InAscii};
use crate::line::{Line, Tone};
use crate::width::cell_width;
use crate::words::NOTHING_NEEDS_YOU;

impl Noted {
    /// The mark in its tone, then the sentence plain: what Markdown and a page say of a
    /// note, and what a terminal draws where it fits.
    pub(in crate::surface) fn said(&self, glyphs: Glyphs) -> Line {
        Line::new()
            .push(Tone::of(self.mark), glyphs.mark(self.mark).to_string())
            .plain(format!(" {}", self.said))
    }
}

impl Terminal {
    /// Where the person is, from the outermost place, the first strong and the rest
    /// plain, and the tally at the row's end: each count with its standing's mark, in its
    /// tone, or, where the runs counted need nothing, `nothing needs you`, muted. A tally
    /// that counts nothing leaves the trail alone. A row too short for both lets the tally's
    /// words go first, its marks and counts staying (`◆ 1 · ▲ 1`), and the trail is never cut
    /// for them; a row too short for the trail beside the marks and counts cuts the trail and
    /// keeps them whole where they fit; a tally wider than the row is cut.
    pub fn trail(&self, trail: &[String], tally: &Tally) -> Line {
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
        let beside =
            |tally: &Line| cell_width(&places.text()) + APART + cell_width(&tally.text()) <= cells;
        let counted = match tally {
            Tally::Uncounted => return places.cut(cells, glyphs),
            Tally::Counted(counts) if counts.is_empty() => {
                Line::new().push(Tone::Muted, NOTHING_NEEDS_YOU)
            }
            Tally::Counted(counts) => match counted(counts, glyphs, TallyWords::Kept) {
                whole if beside(&whole) => whole,
                _ => counted(counts, glyphs, TallyWords::GivenWay),
            },
        };
        let counted = counted.cut(cells, glyphs);
        let room = cells.saturating_sub(cell_width(&counted.text()) + APART);
        at_ends(places.cut(room, glyphs), Some(counted), cells)
    }

    /// What `about` says across a screen in at most `rows` rows: its blocks one under
    /// another, no blank row between two; where the rows fall short, each ribbon draws its
    /// strokes alone, its names giving way first, then the band is cut from its last row.
    pub fn across(&self, about: &Doc, rows: usize) -> Vec<Line> {
        let blocks = about.blocks();
        let mut drawn: Vec<Vec<Line>> = blocks.iter().map(|block| self.block(block)).collect();
        let mut taken: usize = drawn.iter().map(Vec::len).sum();
        for (block, lines) in blocks.iter().zip(drawn.iter_mut()) {
            let Block::Ribbon(ribbon) = block else {
                continue;
            };
            if taken <= rows {
                break;
            }
            taken = taken - lines.len() + 1;
            *lines = vec![ribbon.strokes(&self.look)];
        }
        let mut band: Vec<Line> = drawn.into_iter().flatten().collect();
        band.truncate(rows);
        band
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

    /// A region's title row: its parts as a desk's title draws them, the first strong where the
    /// keys act on the region and muted elsewhere, and its edge, muted, at the row's far end. A
    /// row too short for both cuts the title and keeps the edge whole where the edge fits; an
    /// edge wider than the row is cut.
    pub fn title(&self, region: &Region, focused: bool) -> Line {
        let glyphs = self.look.glyphs;
        let cells = self.look.width.cells();
        let first = match focused {
            true => Tone::Strong,
            false => Tone::Muted,
        };
        let Some(edge) = &region.edge else {
            return title_parts(&region.title, cells, glyphs, first);
        };
        let edge = Line::new()
            .push(Tone::Muted, edge.as_str())
            .cut(cells, glyphs);
        let room = cells.saturating_sub(cell_width(&edge.text()) + APART);
        at_ends(
            title_parts(&region.title, room, glyphs, first),
            Some(edge),
            cells,
        )
    }

    /// The note's row: its mark in its tone and its sentence plain, untinted, cut at the
    /// row's end with the ellipsis.
    pub fn noted(&self, noted: &Noted) -> Line {
        noted
            .said(self.look.glyphs)
            .cut(self.look.width.cells(), self.look.glyphs)
    }
}

/// Whether a tally says each count's words, `1 needs you`, or gave them way to the trail and
/// says its figure alone, `1`. Not a component's [`Said`](crate::components::Said), what a
/// session said.
#[derive(Clone, Copy)]
enum TallyWords {
    Kept,
    GivenWay,
}

/// Each of `counts` with its standing's mark, in its tone, with its words where `words`
/// keeps them, a muted separator between two.
fn counted(counts: &[Count], glyphs: Glyphs, words: TallyWords) -> Line {
    let sep = glyphs.spaced_sep();
    counts
        .iter()
        .enumerate()
        .fold(Line::new(), |line, (at, count)| {
            let mark = count.standing.mark();
            let line = match at {
                0 => line,
                _ => line.push(Tone::Muted, sep.as_str()),
            };
            let figure = match words {
                TallyWords::Kept => count.said(),
                TallyWords::GivenWay => count.runs.to_string(),
            };
            line.push(Tone::of(mark), format!("{} {figure}", glyphs.mark(mark)))
        })
}

/// A title's parts in `cells`, as a region's title and a desk's draw them: as many leading
/// parts as fit stand whole, the first in `first` and the rest muted after the separator. A
/// first part too wide for the cells is cut.
pub(super) fn title_parts(parts: &[String], cells: usize, glyphs: Glyphs, first: Tone) -> Line {
    let sep = glyphs.spaced_sep();
    let mut width = 0;
    let kept = parts
        .iter()
        .enumerate()
        .take_while(|(at, part)| {
            let joint = if *at == 0 { 0 } else { cell_width(&sep) };
            width += joint + cell_width(part);
            width <= cells
        })
        .count();
    match parts.first() {
        None => Line::new(),
        Some(name) if kept == 0 => Line::new().push(first, name.as_str()).cut(cells, glyphs),
        Some(_) => {
            parts[..kept]
                .iter()
                .enumerate()
                .fold(Line::new(), |line, (at, part)| match at {
                    0 => line.push(first, part.as_str()),
                    _ => line
                        .push(Tone::Muted, sep.as_str())
                        .push(Tone::Muted, part.as_str()),
                })
        }
    }
}

#[cfg(test)]
mod tests;

impl Terminal {
    /// What this terminal names its window while `screen` stands: the screen's title as it
    /// draws the screen, in ASCII where its glyphs are, as a terminal that names no UTF-8
    /// locale draws the screen's rows; none leaves the window its own.
    pub fn window_title(&self, screen: &Screen) -> Option<String> {
        match self.look.glyphs {
            Glyphs::Ascii => screen.title.in_ascii(),
            Glyphs::Unicode => screen.title.clone(),
        }
    }
}
