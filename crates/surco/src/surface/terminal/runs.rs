//! A field of runs at a terminal, a row to each, its columns lined up down the field: where
//! the run stands, its handle in a column as wide as the widest, what it runs, its furrow in a
//! column of sixteen cells that starts at the same cell on every row, what holds it in what is
//! left, cut there with the ellipsis, and its edge whole at the far end, two cells after what
//! holds it, an age right-aligned in no fewer cells than an age below a hundred days takes.
//!
//! Where the row stands decides two columns. In a screen's region (a width that cuts) the row
//! names the mode alone in ten cells, since the glance beside the list names the workflow of
//! the run pointed at, and the furrow's column is sixteen cells, as the design's field draws
//! them. On a printed page, where no glance stands, each column is as wide as what its rows
//! hold: the workflow with its mode, in a column as wide as the widest name, up to a quarter
//! of the line; and the furrow's column as wide as the widest furrow, sixteen cells at most, so
//! what holds a run takes the cells a wider column would leave blank.
//!
//! The lists of runs a document holds, a field's groups, line their columns up with one
//! another (`LinedUp`): the handle, the name, the furrow and the edge take the widest of every
//! row among them, so the furrows and what holds each run start at one cell down the whole
//! page.
//!
//! The furrow is a page's furrow in that column (`Terminal::furrow`): two cells a node at
//! most, a gap between two strokes where there are two a node, and where a run has more nodes
//! than cells, each cell takes the most urgent word of the nodes it covers. A row too short
//! for all of it cuts what holds the run first, down to sixteen cells; past that the furrow
//! gives up its column, since what holds a run matters more than its picture.

use super::{Terminal, APART};
use crate::components::{Edge, RunLine, Runs};
use crate::doc::Block;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, truncate, INDENT};

/// The cells a run's mode takes with the gap after it in a screen's region: ten, as the
/// design's field sets the column, `standard` and two cells.
const MODE_COLUMN: usize = 10;

/// The cells of a run's furrow column in a screen's region, and the most a page gives it:
/// sixteen, as the design's field sets it, room for a workflow of fifteen nodes at a cell a
/// node.
const FURROW_COLUMN: usize = 16;

/// The fewest cells an edge takes: an age below a hundred days, `40d`, as the design's field
/// sets its ages.
const AGE_CELLS: usize = 3;

/// The fewest cells a row's sentence keeps before the furrow gives up
/// its column: what holds a run matters more than its picture.
const SAID_LEAST: usize = 16;

/// The widths the lists of runs of one document share, so a field's groups line their columns
/// up down the page: the widest handle, name, furrow and edge among all their rows. Not
/// [`Columns`], what one line is laid out with.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct LinedUp {
    handle: usize,
    name: usize,
    furrow: usize,
    edge: usize,
}

impl LinedUp {
    /// What the lists of runs among `blocks` share; nothing for blocks that hold none.
    pub(super) fn among(blocks: &[Block]) -> LinedUp {
        blocks
            .iter()
            .filter_map(|block| match block {
                Block::Runs(runs) => Some(&runs.rows),
                _ => None,
            })
            .flatten()
            .fold(LinedUp::default(), |lined, row| LinedUp {
                handle: lined.handle.max(cell_width(&row.handle)),
                name: lined.name.max(cell_width(&row.name())),
                furrow: lined
                    .furrow
                    .max(super::page::furrow_cells(row.furrow.len(), FURROW_COLUMN)),
                edge: lined.edge.max(cell_width(row.edge.text())),
            })
    }
}

/// The cells a field's columns take on one line.
struct Columns {
    handle: usize,
    /// The cells what it runs takes, with the gap after it.
    name: usize,
    /// Whether the row names the workflow with its mode, as a page does, or the mode alone,
    /// as a screen's region does.
    named: bool,
    furrow: usize,
    said: usize,
    edge: usize,
}

impl Runs {
    /// Each run on a row, its handle, what it runs, its furrow, what holds
    /// it and its edge each in a column of their own, lined up with every
    /// list of runs of its document as `lined` says.
    pub(super) fn lines(&self, look: &Look, lined: LinedUp) -> Vec<Line> {
        let columns = Columns::of(lined, look);
        self.rows
            .iter()
            .map(|row| row.line(&columns, look))
            .collect()
    }
}

impl Columns {
    /// The columns on a line of `look`'s width: the handle and the edge as wide as `lined`
    /// says; in a region the mode's ten cells and the furrow's sixteen, or on a page the name
    /// as wide as `lined` says up to a quarter of the line, and two cells, and the furrow as
    /// wide as `lined` says; a cell after the furrow; and what holds each run in what is left —
    /// the furrow first to go when that is too little.
    fn of(lined: LinedUp, look: &Look) -> Columns {
        let width = look.width.cells();
        let named = !look.width.cuts();
        let (name, furrow) = match named {
            true => (lined.name.min(width / 4) + APART, lined.furrow),
            false => (MODE_COLUMN, FURROW_COLUMN),
        };
        let edge = lined.edge.max(AGE_CELLS);
        // The pointer and the picked mark, the run's mark and a space, the handle and a
        // space, the name's column, and the cells before the edge.
        let taken = cell_width(INDENT) + 2 + lined.handle + 1 + name + APART + edge;
        let furrow = match furrow > 0 && width.saturating_sub(taken + furrow + 1) >= SAID_LEAST {
            true => furrow,
            false => 0,
        };
        Columns {
            handle: lined.handle,
            name,
            named,
            furrow,
            said: width.saturating_sub(taken + furrow + usize::from(furrow > 0)),
            edge,
        }
    }
}

impl RunLine {
    /// The run's row in `columns`: a row a person picks from, the row pointed at led by its
    /// pointer's cell and standing on the selection to its region's edge ([`Line::pointed`]'s
    /// look); a row picked carries the picked mark.
    fn line(&self, columns: &Columns, look: &Look) -> Line {
        let mark = self.standing.mark();
        let picked = match self.marked {
            true => Line::new().push(Tone::Strong, look.glyphs.picked().to_string()),
            false => Line::new().plain(" "),
        };
        let name = match columns.named {
            true => self.name(),
            false => self.mode.clone(),
        };
        // A name too wide for its column is cut a cell short of it, so a cell parts it from
        // the furrow.
        let mut line = Line::new()
            .then(Line::pointer(self.pointed, look.glyphs))
            .then(picked)
            .push(Tone::of(mark), look.glyphs.mark(mark).to_string())
            .plain(" ")
            .push(
                Tone::Strong,
                format!("{:<width$}", self.handle, width = columns.handle),
            )
            .plain(" ")
            .push(Tone::Muted, truncate(&name, columns.name - 1, look.glyphs))
            .plain(" ");
        if columns.furrow > 0 {
            let furrow = Terminal::on(*look).furrow(&self.furrow, columns.furrow);
            let drawn = cell_width(&furrow.text());
            line = line
                .then(furrow)
                .plain(" ".repeat(columns.furrow - drawn + 1));
        }
        let edge = match &self.edge {
            Edge::Age(age) => Line::new().push(Tone::Muted, age.as_str()),
            Edge::Command(command) => Line::new().push(Tone::Strong, command.as_str()),
        };
        let room = columns.edge - cell_width(self.edge.text());
        let line = line
            .plain(truncate(&self.said, columns.said, look.glyphs))
            .plain(" ".repeat(APART + room))
            .then(edge);
        line.on_the_selection(self.pointed)
    }
}

#[cfg(test)]
mod tests;
