//! A list of keys at a terminal: its sentence muted, a blank row, then a key a row, the key
//! strong in a column of seven cells and what it does after it, each cut at the row's end.
//! A key that leaves no cell beside it in the column widens the column to the widest key and
//! two cells, for every row, so what each does stands in one column.
//!
//! Held in a region, as help is in its box, it keeps two blank rows at its foot. Short of
//! rows, the blank rows give way first, those at its foot before the one under its sentence;
//! then the keys' window, from the key the person scrolled to, counting what it hides above
//! and under it where the keys stand. Its sentence stays.

use crate::components::{KeyList, Laid, Sight};
use crate::line::{Hidden, Line, Tone};
use crate::look::Look;
use crate::width::cell_width;

/// The cells a key and the space after it take, as the design's help sets them: `enter` and
/// two cells.
const KEY_COLUMN: usize = 7;

/// The cells between the widest key and what it does, where the widest key widens the
/// column: two, as the design's help leaves after `enter`.
const KEY_GAP: usize = 2;

/// The blank rows under the last key where the rows hold them, as the design's help leaves
/// at its foot.
const FOOT: usize = 2;

impl KeyList {
    /// The list whole, as a document draws it: its sentence, a blank row, then a key a row.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let said = self.said_line(look);
        let blank = said.as_ref().map(|_| Line::new());
        said.into_iter()
            .chain(blank)
            .chain(self.rows(look))
            .collect()
    }

    /// The list held in `rows` rows, from the key `sight` asks for, with what was laid out of
    /// its keys for the next key to scroll from.
    pub(in crate::surface::terminal) fn held(
        &self,
        look: &Look,
        sight: Sight,
        rows: usize,
    ) -> (Vec<Line>, Laid) {
        let said: Vec<Line> = self.said_line(look).into_iter().collect();
        let keys = self.rows(look);
        let whole = said.len() + keys.len();
        if whole <= rows {
            let spare = rows - whole;
            let under = said.len().min(spare);
            let foot = FOOT.min(spare - under);
            let blank = |count: usize| std::iter::repeat_n(Line::new(), count);
            let laid = Laid {
                top: 0,
                page: keys.len(),
                most: 0,
            };
            let lines = said
                .into_iter()
                .chain(blank(under))
                .chain(keys)
                .chain(blank(foot))
                .collect();
            return (lines, laid);
        }
        let (window, laid) = windowed(keys, sight.top, rows.saturating_sub(said.len()), look);
        (said.into_iter().chain(window).take(rows).collect(), laid)
    }

    /// Its sentence, muted, cut at the row's end.
    fn said_line(&self, look: &Look) -> Option<Line> {
        let said = self.said.as_deref()?;
        Some(
            Line::new()
                .push(Tone::Muted, said)
                .cut(look.width.cells(), look.glyphs),
        )
    }

    /// A key a row: the key strong in the column, what it does after it, cut at the row's end.
    fn rows(&self, look: &Look) -> Vec<Line> {
        let spelled: Vec<&str> = self
            .keys
            .0
            .iter()
            .map(|binding| binding.keys.spelled(look.glyphs))
            .collect();
        let column = key_column(&spelled);
        spelled
            .iter()
            .zip(&self.keys.0)
            .map(|(key, binding)| {
                let pad = " ".repeat(column - cell_width(key));
                Line::new()
                    .push(Tone::Strong, format!("{key}{pad}"))
                    .plain(binding.does.as_str())
                    .cut(look.width.cells(), look.glyphs)
            })
            .collect()
    }
}

/// The cells a column of `keys`, as spelled, takes with the space after them: seven, or the
/// widest key and two cells where a key leaves no cell beside it in seven. Every list of keys
/// a key a row lays its keys so, help's and a glance's acts alike.
pub(super) fn key_column(keys: &[&str]) -> usize {
    let widest = keys.iter().map(|key| cell_width(key)).max().unwrap_or(0);
    match widest < KEY_COLUMN {
        true => KEY_COLUMN,
        false => widest + KEY_GAP,
    }
}

/// `keys` in `budget` rows from the key `top`: a row that counts the keys above them where
/// it starts past the first, and one that counts those under them where it ends before the
/// last; with too few rows for a count, the keys alone.
fn windowed(keys: Vec<Line>, top: usize, budget: usize, look: &Look) -> (Vec<Line>, Laid) {
    let len = keys.len();
    let most = (len + 1).saturating_sub(budget);
    let top = top.min(most);
    let laid = Laid {
        top,
        page: budget,
        most,
    };
    let counted = |count: usize, side: Hidden| Line::hidden(count, side, look.glyphs);
    if budget < 2 {
        return (keys.into_iter().skip(top).take(budget).collect(), laid);
    }
    let above = counted(top, Hidden::Above);
    let room = budget - usize::from(above.is_some());
    let end = match top + room >= len {
        true => len,
        false => top + room - 1,
    };
    let under = counted(len - end, Hidden::Under);
    let window = above
        .into_iter()
        .chain(keys.into_iter().take(end).skip(top))
        .chain(under)
        .collect();
    (window, laid)
}
