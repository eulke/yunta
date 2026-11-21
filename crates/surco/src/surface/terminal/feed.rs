//! A feed on a terminal: a row a line, each cut at the edge it is drawn at rather than folded
//! onto the line under it, between the rows that count what stands above and under it. A row
//! is cut as every line is, so a cut never falls inside a number or right after one.
//!
//! In a pane a feed shows a window of its rows: at its end, the row that counts those above
//! it and the newest rows under that; scrolled back, the rows from the place asked for between
//! the rows that count those above and under. A window too short to hold a count beside a row
//! lets its counts go before its rows, the newest last.
//!
//! Where a feed says what each row is about, each row it shows opens with its subject in a
//! column as wide as the widest subject among the rows shown and [`APART`] more: a window
//! measures the rows in its sight, any other drawing every row it holds.

use std::ops::Range;

use crate::components::{Feed, Laid, Sight};
use crate::glyphs::Glyphs;
use crate::line::{Hidden, Line, Tone};
use crate::look::Look;
use crate::width::cell_width;

/// The cells that part a row's subject from its words: two, as a feed's call parts its verb
/// from its object.
const APART: usize = 2;

impl Feed {
    /// Each row on a line of its own, cut at the stream's width and never folded.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        self.said(look.glyphs)
            .iter()
            .map(|row| row.cut(look.width.cells(), look.glyphs))
            .collect()
    }

    /// The rows in sight, after a row that counts those above them and before one that counts
    /// those under them, each where its count is not zero and muted, as a reader skims past
    /// it: what every surface says of a feed, its ellipsis drawn in `glyphs`.
    pub(in crate::surface) fn said(&self, glyphs: Glyphs) -> Vec<Line> {
        Line::hidden(self.before, Hidden::Above, glyphs)
            .into_iter()
            .chain(self.subjected(0..self.rows.len()))
            .chain(Line::hidden(self.after, Hidden::Under, glyphs))
            .collect()
    }

    /// The rows it holds at `held`, each opened by what it is about where the feed says it,
    /// in a column as wide as the widest subject among them and [`APART`] more, in the tone
    /// its row opens in; the rows as they are where it says none.
    fn subjected(&self, held: Range<usize>) -> Vec<Line> {
        let rows = &self.rows[held.clone()];
        if self.subjects.is_empty() {
            return rows.to_vec();
        }
        let subject = |at: usize| self.subjects.get(at).map_or("", String::as_str);
        let widest = held.clone().map(|at| cell_width(subject(at))).max();
        let column = widest.unwrap_or(0) + APART;
        held.zip(rows)
            .map(|(at, row)| {
                let tone = row.spans().first().map_or(Tone::Plain, |span| span.tone);
                let pad = column.saturating_sub(cell_width(subject(at)));
                let opening = format!("{}{}", subject(at), " ".repeat(pad));
                Line::new().push(tone, opening).then(row.clone())
            })
            .collect()
    }

    /// How many rows the whole feed holds: those it holds, and those above and under them.
    pub(in crate::surface) fn whole(&self) -> usize {
        self.before + self.rows.len() + self.after
    }

    /// What a window `rows` high shows of the feed from the row `sight` asks for, counted
    /// from the whole feed's first row, each row cut at the stream's width; and what was laid
    /// out of it, counted the same way, a page of it the rows in sight at its end. Of the rows
    /// it would show, only those the feed holds show, the rest counted with those above or
    /// under them.
    pub(in crate::surface) fn windowed(
        &self,
        look: &Look,
        sight: Sight,
        rows: usize,
    ) -> (Vec<Line>, Laid) {
        if rows == 0 {
            return (Vec::new(), Laid::default());
        }
        let whole = self.whole();
        let (first, last, most) = in_sight(whole, sight.top, rows);
        let held_end = self.before + self.rows.len();
        let first = first.clamp(self.before, held_end);
        let last = last.clamp(first, held_end);
        let (cells, glyphs) = (look.width.cells(), look.glyphs);
        let shown = self
            .subjected(first - self.before..last - self.before)
            .into_iter()
            .map(|row| row.cut(cells, glyphs));
        let mut above = Line::hidden(first, Hidden::Above, glyphs);
        let mut under = Line::hidden(whole - last, Hidden::Under, glyphs);
        let counted = usize::from(above.is_some()) + usize::from(under.is_some());
        // The counts give way before a row does: the one under first, the newest rows' side.
        if (last - first) + counted > rows {
            under = None;
        }
        if (last - first) + usize::from(above.is_some()) > rows {
            above = None;
        }
        let lines = above
            .into_iter()
            .map(|above| above.cut(cells, glyphs))
            .chain(shown)
            .chain(under.map(|under| under.cut(cells, glyphs)))
            .collect();
        let laid = Laid {
            top: first,
            page: rows.saturating_sub(1),
            most,
        };
        (lines, laid)
    }
}

/// Which of a feed's `whole` rows a window `rows` high shows from the row `top`: the first
/// and the one after the last, and the last first row it can show. Past that row the window
/// is at its end: the row that counts those above, then the newest rows. Short of it, the
/// rows from `top` on between the rows that count those above and under, at least one.
fn in_sight(whole: usize, top: usize, rows: usize) -> (usize, usize, usize) {
    if whole <= rows {
        return (0, whole, 0);
    }
    let most = whole - rows.saturating_sub(1).max(1);
    let top = top.min(most);
    if top == most {
        return (top, whole, most);
    }
    let counts = usize::from(top > 0) + 1;
    let in_sight = rows.saturating_sub(counts).max(1);
    (top, top + in_sight, most)
}
