//! A feed on a terminal: a row a line, each cut at the edge it is drawn at rather than folded
//! onto the line under it, between the rows that count what stands above and under it.
//!
//! A cut never falls inside a number or right after one: a number cut short reads as a
//! smaller one, so the cut falls before it and the number goes with what the cut leaves out.

use crate::components::Feed;
use crate::glyphs::Glyphs;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::cell_width;

/// The word after the count of the rows that stand above a feed's rows in sight.
const ABOVE: &str = "above";

/// The word after the count of the rows that stand under a feed's rows in sight.
const UNDER: &str = "under";

impl Feed {
    /// Each row on a line of its own, cut at the stream's width and never folded.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        self.said(look.glyphs)
            .iter()
            .map(|row| cut(row, look.width.cells(), look.glyphs))
            .collect()
    }

    /// The rows in sight, after a row that counts those above them and before one that counts
    /// those under them, each where its count is not zero and muted, as a reader skims past
    /// it: what every surface says of a feed, its ellipsis drawn in `glyphs`.
    pub(in crate::surface) fn said(&self, glyphs: Glyphs) -> Vec<Line> {
        let hidden = |count: usize, side: &str| {
            (count > 0).then(|| {
                Line::new().push(Tone::Muted, format!("{} {count} {side}", glyphs.ellipsis()))
            })
        };
        hidden(self.before, ABOVE)
            .into_iter()
            .chain(self.rows.iter().cloned())
            .chain(hidden(self.after, UNDER))
            .collect()
    }
}

/// `row` inside `cells`, cut as [`Line::cut`] cuts it, but before a number the cut would split
/// or end on. The ellipsis takes the tone and fill of the span it ends.
fn cut(row: &Line, cells: usize, glyphs: Glyphs) -> Line {
    let text = row.text();
    if cell_width(&text) <= cells {
        return row.clone();
    }
    let cut = row.cut(cells, glyphs);
    let kept = cut
        .text()
        .strip_suffix(glyphs.ellipsis())
        .map_or(0, str::len);
    let end = clear_of_numbers(&text, kept);
    if end == kept {
        return cut;
    }
    let kept = row.slice(0..end);
    let ends = kept.spans().last().or(row.spans().first());
    let ellipsis = glyphs.ellipsis();
    match ends.map(|span| (span.tone, span.fill)) {
        Some((tone, Some(fill))) => kept.push_filled(tone, fill, ellipsis),
        Some((tone, None)) => kept.push(tone, ellipsis),
        None => kept.plain(ellipsis),
    }
}

/// Where a cut that keeps `text[..end]` falls so that it splits no number and ends on none:
/// before the number it would split or end on, again before the one that leaves it ending
/// on, with the spaces before the cut left out. Each turn moves the cut back, so it ends.
fn clear_of_numbers(text: &str, end: usize) -> usize {
    let mut end = end;
    loop {
        let kept = text[..end].trim_end();
        let splits = kept.len() == end
            && kept.ends_with(in_a_number)
            && text[end..].starts_with(|c: char| c.is_ascii_digit());
        if !(kept.ends_with(|c: char| c.is_ascii_digit()) || splits) {
            return kept.len();
        }
        end = number_start(kept);
    }
}

/// Where the number `kept` ends in starts: the letters, digits and stops (`13.7M`, `1m31s`,
/// `10:05`) that run back from its end, from the first letter or digit among them, since a
/// stop opens no number. Always before the end of `kept`, which ends in one of them.
fn number_start(kept: &str) -> usize {
    let run = kept
        .char_indices()
        .rev()
        .take_while(|(_, c)| in_a_number(*c))
        .last()
        .map_or(kept.len(), |(at, _)| at);
    kept[run..]
        .find(char::is_alphanumeric)
        .map_or(run, |at| run + at)
}

/// Whether `c` can stand inside a number as Yunta writes one: a letter or a digit, or a stop
/// that parts two of them.
fn in_a_number(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | ',' | ':')
}
