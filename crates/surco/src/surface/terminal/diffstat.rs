//! A change, file by file, on a terminal: the path, what it adds and takes out,
//! and a bar of each against the file that moves most. The bar goes first where
//! there is no room, and the figures say what it says.

use crate::bars::{bar_cells, fine_bar};
use crate::components::{Diffstat, FileRow};
use crate::figures::signed_lines;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, middle_cut, INDENT};

/// The cells between two columns of a row: one would read as a word's space,
/// and a figure beside a path has to read as a column of its own.
const GAP: usize = 2;

impl Diffstat {
    /// Each file's row, and what the rest come to.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let columns = Columns::of(self, look);
        let room = look.width.cells().saturating_sub(cell_width(INDENT));
        self.files
            .iter()
            .map(|file| columns.row(file, self.scale(), look))
            .chain(self.more.iter().flat_map(|more| {
                Line::new()
                    .push(Tone::Muted, more.said(look.glyphs))
                    .wrap(room)
                    .into_iter()
                    .map(|line| line.under(INDENT))
            }))
            .collect()
    }
}

/// How wide each column of a row is, on a line `look` is as wide as.
struct Columns {
    path: usize,
    added: usize,
    removed: usize,
    /// The cells the row's two bars share.
    bars: usize,
}

impl Columns {
    /// The figures whole, the path in what they leave, and the bars in what
    /// is left after both: the bars go first, then the path is cut.
    fn of(stat: &Diffstat, look: &Look) -> Self {
        let figures: Vec<[String; 2]> = stat
            .files
            .iter()
            .map(|file| signed_lines(file.added, file.removed, look.glyphs))
            .collect();
        let widest = |at: usize| {
            figures
                .iter()
                .map(|said| cell_width(&said[at]))
                .max()
                .unwrap_or(0)
        };
        let (added, removed) = (widest(0), widest(1));
        let longest = stat
            .files
            .iter()
            .map(|file| cell_width(&file.path))
            .max()
            .unwrap_or(0);
        let width = look.width.cells();
        let both = added + removed + 1;
        let path = longest.min(width.saturating_sub(cell_width(INDENT) + GAP + both));
        let bars = bar_cells(width, cell_width(INDENT) + path + GAP + both + GAP);
        Columns {
            path,
            added,
            removed,
            bars,
        }
    }

    /// One file's row. What it has no line of it says nothing of, and a line
    /// ends where its last word does. Each bar rounds up to a whole cell, so
    /// each is drawn against one cell less than the two share, or two together
    /// could take one cell more than they have.
    fn row(&self, file: &FileRow, scale: u64, look: &Look) -> Line {
        let glyphs = look.glyphs;
        let [added, removed] = signed_lines(file.added, file.removed, glyphs);
        let bar = |count: u64| {
            fine_bar(
                count as f64,
                scale as f64,
                self.bars.saturating_sub(1),
                glyphs,
            )
            .drawn()
        };
        trimmed(
            Line::new()
                .plain(INDENT)
                .plain(middle_cut(&file.path, self.path, glyphs))
                .plain(" ".repeat(GAP))
                .push(Tone::Done, format!("{added:>width$}", width = self.added))
                .plain(" ")
                .push(
                    Tone::Failed,
                    format!("{removed:<width$}", width = self.removed),
                )
                .plain(" ".repeat(GAP))
                .push(Tone::Done, bar(file.added))
                .push(Tone::Failed, bar(file.removed)),
        )
    }
}

/// `line` up to its last word: what its columns were padded with after that is
/// no part of it.
fn trimmed(line: Line) -> Line {
    let text = line.text();
    line.slice(0..text.trim_end().len())
}

#[cfg(test)]
mod tests;
