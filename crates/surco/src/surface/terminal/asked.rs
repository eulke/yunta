//! A line asked and a list picked from, at a terminal: the question and the boxed row
//! an answer is typed on, with the cursor on it; the list what was typed leaves, in the
//! rows it is given.
//!
//! A list is narrowed by typing, so it is no line asked and draws no box: its first row
//! says what was typed after a pointer of its own, a cell in from the rows' pointer, or
//! that typing filters; a blank row parts it from the rows. Each row is one row, its name
//! in a column three cells past the widest name and what it says beside it cut at the
//! row's end. A list whose rows carry keys, as the palette's do, stands in three columns
//! instead where its width holds them, as a palette's box does on a terminal of 100
//! columns: what a row does, its key strong, then what it says, its command, muted; on a
//! narrower one, and in a box over a screen narrower than that, what a row does alone. A list longer than its rows keeps the rows around
//! its pointer in sight and says how many it hides above them and under them, where its
//! names stand. Its keys stand under it only where it says its own, folded as a legend
//! folds, between two keys and never inside one: a screen that holds them leaves them out.
//! Short of rows, the blank row gives way first, then the rows the window leaves, then the
//! keys.

use super::card::options::Said;
use super::card::Placed;
use crate::components::{ChoiceList, ChoiceRow, Choices, TypedAnswer};
use crate::glyphs::Glyphs;
use crate::line::{Hidden, Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, id_column, truncate};

/// The cells between a list's widest name and what its rows say beside their names: the
/// design's lists, a new run's workflows and the shapes of documents among them, stand
/// them three apart, a cell wider than a card's options.
const NAME_GAP: usize = 3;

/// The cells the row a list is narrowed on stands in from the column its rows' pointer
/// moves along, so its own pointer is never read as a row's.
const FILTER_IN: usize = 1;

/// The cells what a row whose list carries keys does takes after its pointer, as the
/// design's palette sets them: its longest act and a cell.
const DOES: usize = 40;

/// The cells a row's key and the space after it take, as the design's palette sets them:
/// `enter` and a cell.
const KEY: usize = 6;

/// The fewest cells a list whose rows carry keys holds its keys and commands in: the
/// palette's box on a terminal of 100 columns, the design's wide one, is 84 cells less its
/// two uprights.
const KEYS_FROM: usize = 82;

impl TypedAnswer {
    /// The question, and under it the boxed row the words are typed on across the
    /// look's width, showing the part of them around the cursor that fits, with the
    /// cursor on it.
    pub fn drawn(&self, look: &Look) -> Placed {
        let typed = self.typed.drawn(look.width.cells(), look.glyphs, None);
        let mut lines = vec![self.asked.clone()];
        let asked = lines.len();
        lines.extend(typed.lines);
        Placed {
            lines,
            cursor: typed.cursor.map(|(row, cell)| (asked + row, cell)),
            laid: None,
        }
    }
}

/// Whether a list whose rows carry keys holds each row's key and command beside what it
/// does: where its width holds them, or never, as a palette's box over a screen narrower
/// than the design's wide one. Not the list's own keys, which stand under its rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Columns {
    WhereTheyFit,
    WordsAlone,
}

impl ChoiceList {
    /// The list in at most `rows` rows: the row it is narrowed on, a blank row, its rows
    /// and its keys, with the cursor where typing lands on its first row. Short of rows the
    /// blank row gives way, then the rows around the pointer are kept in what the keys
    /// leave, and with too few rows for the least of them beside the keys, the keys give way
    /// too; the row it is narrowed on stands as long as the list does.
    pub fn drawn(&self, look: &Look, rows: usize) -> Placed {
        self.placed(look, rows, Columns::WhereTheyFit)
    }

    /// The list as [`ChoiceList::drawn`] holds it, its rows' keys and commands standing as
    /// `columns` says.
    pub(super) fn placed(&self, look: &Look, rows: usize, columns: Columns) -> Placed {
        let lines = self.laid_out(look, rows, columns);
        let cells = look.width.cells();
        let lands = filter_cursor(self.filter.as_deref(), look.glyphs, cells);
        Placed {
            cursor: lines.first().map(|_| (0, lands)),
            lines,
            laid: None,
        }
    }

    /// The list's lines as [`ChoiceList::drawn`] lays them out, its rows' keys and commands
    /// standing as `columns` says.
    fn laid_out(&self, look: &Look, rows: usize, columns: Columns) -> Vec<Line> {
        let cells = look.width.cells();
        let keys = self.legend.folded(look, cells);
        let filter = filter_row(self.filter.as_deref(), look.glyphs);
        let choices = self.choices.as_ref();
        let listed = |budget: usize| listed(choices, look, (cells, columns), budget);
        let whole = listed(usize::MAX);
        match 2 + whole.len() + keys.len() <= rows {
            true => std::iter::once(filter)
                .chain(std::iter::once(Line::new()))
                .chain(whole)
                .chain(keys)
                .collect(),
            false => {
                let window = listed(rows.saturating_sub(1 + keys.len()));
                match 1 + window.len() + keys.len() <= rows {
                    true => std::iter::once(filter).chain(window).chain(keys).collect(),
                    false => std::iter::once(filter)
                        .chain(listed(rows.saturating_sub(1)))
                        .take(rows)
                        .collect(),
                }
            }
        }
    }
}

/// The row a list is narrowed on: a cell in from the rows' pointer, a pointer of its own,
/// muted, then what was typed, strong, or where nothing was, that typing filters, faint.
pub(super) fn filter_row(filter: Option<&str>, glyphs: Glyphs) -> Line {
    let opening = filter_opening(glyphs);
    match filter {
        Some(typed) => opening.push(Tone::Strong, typed),
        None => opening.push(Tone::Faint, "type to filter"),
    }
}

/// The cell typing lands on in the row a list is narrowed on, `room` cells wide: the first
/// of `type to filter` while nothing is typed, the one after the words typed otherwise, and
/// never past the row's last cell. Every printable key types there, so the cursor stands
/// there wherever the list is held.
pub(super) fn filter_cursor(filter: Option<&str>, glyphs: Glyphs, room: usize) -> usize {
    let lands = cell_width(&filter_opening(glyphs).text()) + filter.map_or(0, cell_width);
    lands.min(room.saturating_sub(1))
}

/// The row a list is narrowed on up to where typing lands: a cell in from the rows'
/// pointer, a pointer of its own, muted, and a space.
fn filter_opening(glyphs: Glyphs) -> Line {
    Line::new()
        .plain(" ".repeat(FILTER_IN))
        .push(Tone::Muted, glyphs.pointer().to_string())
        .plain(" ")
}

/// A list's rows in at most `budget` lines of `room` cells, each on one row, a row's key
/// and command standing as `columns` says: every row where they fit, else the rows around
/// the pointer with how many it hides above and under them, the counts first to give way
/// where the rows leave them none; or, where the filter left none, that nothing matches.
/// Each count and that sentence stand where the names do.
pub(super) fn listed(
    choices: Option<&Choices>,
    look: &Look,
    (room, columns): (usize, Columns),
    budget: usize,
) -> Vec<Line> {
    let glyphs = look.glyphs;
    let names_column = Line::pointed(false, glyphs).text();
    let Some(choices) = choices else {
        return vec![Line::new()
            .push(Tone::Muted, "nothing matches")
            .under(&names_column)];
    };
    let name = id_column(choices.rows.iter().map(|row| row.name.as_str())) + NAME_GAP;
    let keyed = choices.keyed();
    let len = choices.rows.len();
    let (start, end) = window(len, choices.at.min(len.saturating_sub(1)), budget);
    let mut above = Line::hidden(start, Hidden::Above, glyphs);
    let mut under = Line::hidden(len - end, Hidden::Under, glyphs);
    let shown = end - start;
    if shown + usize::from(above.is_some()) + usize::from(under.is_some()) > budget {
        above = None;
    }
    if shown + usize::from(under.is_some()) > budget {
        under = None;
    }
    let rows = choices.rows[start..end]
        .iter()
        .enumerate()
        .flat_map(|(offset, row)| {
            let pointed = start + offset == choices.at;
            match keyed {
                true => vec![keyed_row(row, pointed, (room, columns), glyphs)],
                false => row.lines(pointed, name, room, glyphs, Said::Cut),
            }
        });
    above
        .map(|count| count.under(&names_column))
        .into_iter()
        .chain(rows)
        .chain(under.map(|count| count.under(&names_column)))
        .collect()
}

/// A row of a list whose rows carry keys, pointed at or not, in `room` cells: what it does
/// in its column, its key strong, and its command muted, where `columns` lets them stand and
/// the room holds them; what it does alone elsewhere. It is cut at the row's end, and says
/// no cost.
fn keyed_row(
    row: &ChoiceRow,
    pointed: bool,
    (room, columns): (usize, Columns),
    glyphs: Glyphs,
) -> Line {
    let does = match pointed {
        true => Tone::Strong,
        false => Tone::Plain,
    };
    let opening = Line::pointed(pointed, glyphs);
    let line = match columns == Columns::WhereTheyFit && room >= KEYS_FROM {
        true => {
            let key = match &row.key {
                Some(key) => {
                    Line::new().push(Tone::Strong, format!("{} ", truncate(key, KEY - 1, glyphs)))
                }
                None => Line::new().plain(" ".repeat(KEY)),
            };
            opening
                .push(does, format!("{} ", truncate(&row.name, DOES - 1, glyphs)))
                .then(key)
                .push(Tone::Muted, row.said.as_deref().unwrap_or_default())
        }
        false => opening.push(does, row.name.as_str()),
    };
    line.cut(room, glyphs)
}

/// The rows of a list of `len` kept in sight around the row `at` in `budget` lines, from
/// its first to past its last: every row where they fit; else as many as leave a line for
/// the count of those hidden at one end, the pointer near it, or at both ends, the pointed
/// row last in sight. At least the pointed row.
fn window(len: usize, at: usize, budget: usize) -> (usize, usize) {
    if len <= budget {
        return (0, len);
    }
    let beside_one_count = budget.saturating_sub(1).max(1);
    if at < beside_one_count {
        return (0, beside_one_count);
    }
    if at >= len - beside_one_count {
        return (len - beside_one_count, len);
    }
    let beside_two = budget.saturating_sub(2).max(1);
    let start = (at + 1).saturating_sub(beside_two);
    (start, start + beside_two)
}
