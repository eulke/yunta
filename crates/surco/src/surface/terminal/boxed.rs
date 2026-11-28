//! What is open over a screen, boxed: the palette, help or a confirmation drawn inside the
//! edges of a box with round corners, its title strong in the top edge a cell after the
//! first corner, its document drawn by the rule a region is drawn by. The box draws no keys:
//! its own stand in the screen's legend row while it is open.
//!
//! What it holds stands between its uprights: words a cell in from the left one, so a line
//! never touches the edge it starts from; a list a person picks from across every cell
//! between them, its pointer in that cell, so the row it stands on is filled from one upright
//! to the other. A row cut at the box's edge meets the right upright, since the cut's mark
//! already says the row goes on. A confirmation is read as a sentence: its words stand a cell
//! inside each upright, so they wrap four cells short of the box, and a blank row stands at
//! its foot, the first of its rows to give way. The cell inside the far upright is lent to a
//! word of a command that cannot be parted, which stands there whole.
//!
//! The box stands over a screen as wide as it is given and as tall as what it holds, up to
//! the rows it is given; an engine of cells says where it stands and how wide. A list whose
//! rows carry keys, as the palette's, holds each row's key and command beside what it does
//! only over the design's wide screen, 100 columns, cut at the box's edge where its room is
//! short of them: a box as wide over a narrower screen holds what each row does alone. A box
//! takes at least its two uprights and the cells around its title: given fewer cells than
//! that it still draws them, wider than it was given.

use super::{Paned, Terminal};
use crate::components::{Laid, Overlay};
use crate::diagram::{Shape, Stroke};
use crate::doc::Block;
use crate::glyphs::Glyphs;
use crate::line::{Line, Tone};
use crate::look::Width;
use crate::width::cell_width;

use super::asked::Columns;

/// The cells between the left upright and the words a box holds: a line inside never
/// touches the edge it starts from.
const INSIDE: usize = 1;

/// The cells an upright takes, at either side of a box.
const UPRIGHT: usize = 1;

/// The cells of the top edge around its title: the corner and a cell before it, a cell and
/// the corner after it at the least.
const AROUND_TITLE: usize = 4;

/// The rows a box's edges take: the top, which holds its title, and the bottom.
const EDGE_ROWS: usize = 2;

impl Terminal {
    /// `overlay` boxed `cells` wide over a screen of this look's width and as tall as what it
    /// holds, at most `rows` high: the title in the top edge, each line inside the uprights,
    /// and its document drawn by the rule a region is drawn by ([`Terminal::pane`]), a list
    /// whose rows carry keys holding them beside what each does only over the design's wide
    /// screen; with what was laid out of it, and where the cursor stands in the box.
    pub fn boxed(&self, overlay: &Overlay, cells: usize, rows: usize) -> Paned {
        let edges = Edges::of(self.look.glyphs);
        let (inset, short_of_the_far) = insets(overlay);
        let room = cells.saturating_sub(2 * UPRIGHT + inset + short_of_the_far);
        let inner_rows = rows.saturating_sub(EDGE_ROWS);
        let held = self.held_in_a_box(overlay, cells, inner_rows);
        let title = Line::new()
            .push(Tone::Strong, overlay.title.as_str())
            .cut(cells.saturating_sub(AROUND_TITLE), self.look.glyphs);
        // A confirmation keeps a blank row at its foot where its rows leave one.
        let foot = (overlay.confirms && held.lines.len() < inner_rows).then(Line::new);
        let mut lines = vec![edges.top(title, cells)];
        lines.extend(
            held.lines
                .into_iter()
                .chain(foot)
                .map(|line| edges.inside(line, inset, room + short_of_the_far)),
        );
        lines.push(edges.bottom(cells));
        lines.truncate(rows);
        Paned {
            lines,
            cursor: held
                .cursor
                .map(|(row, cell)| (row + 1, cell + UPRIGHT + inset)),
            laid: held.laid,
        }
    }

    /// What `overlay` holds inside a box `cells` wide, in at most `rows` rows between its
    /// edges: its document drawn by the rule a region is drawn by, a list alone, as the
    /// palette's, laid out by the screen it stands over.
    fn held_in_a_box(&self, overlay: &Overlay, cells: usize, rows: usize) -> Paned {
        let (inset, short_of_the_far) = insets(overlay);
        let room = cells.saturating_sub(2 * UPRIGHT + inset + short_of_the_far);
        let inside = self.at_width(Width::exact(room).lending(short_of_the_far));
        match overlay.doc.blocks() {
            [Block::ChoiceList(list)] => {
                let placed = list.placed(&inside.look, rows, Columns::over(self));
                let mut lines = placed.lines;
                lines.truncate(rows);
                Paned {
                    lines,
                    cursor: placed.cursor.filter(|(row, _)| *row < rows),
                    laid: Laid::default(),
                }
            }
            _ => inside.paned(&overlay.doc, overlay.sight, rows),
        }
    }
}

/// The blank cells `overlay`'s box keeps inside its uprights, before what it holds and after
/// it: a cell before words, none before a list picked from, whose pointer takes that cell;
/// a cell after a confirmation's sentence, none after anything else.
fn insets(overlay: &Overlay) -> (usize, usize) {
    let before = match overlay.doc.prompt_at() {
        Some(_) => 0,
        None => INSIDE,
    };
    let after = match overlay.confirms {
        true => INSIDE,
        false => 0,
    };
    (before, after)
}

/// The marks a box is drawn in, in a glyph set: its four round corners, the level stroke of
/// its top and bottom, and the upright stroke of its sides.
struct Edges {
    corners: [char; 4],
    level: String,
    upright: String,
}

impl Edges {
    fn of(glyphs: Glyphs) -> Self {
        Edges {
            corners: glyphs.corners(Shape::Round),
            level: glyphs
                .joint(false, false, true, true, Stroke::Open)
                .to_string(),
            upright: glyphs
                .joint(true, true, false, false, Stroke::Open)
                .to_string(),
        }
    }

    /// The top edge, `cells` wide, with `title` in it a cell after its first corner.
    fn top(&self, title: Line, cells: usize) -> Line {
        let [left, right, ..] = self.corners;
        let after = cells.saturating_sub(cell_width(&title.text()) + AROUND_TITLE);
        Line::new()
            .push(Tone::Muted, left.to_string())
            .plain(" ")
            .then(title)
            .plain(" ")
            .push(Tone::Muted, format!("{}{right}", self.level.repeat(after)))
    }

    /// `line` between the uprights, `inset` cells in from the left one and filled out to
    /// `room` cells so the far upright stands in line with the rest; a row on the selection
    /// stands on it to the far upright.
    fn inside(&self, line: Line, inset: usize, room: usize) -> Line {
        let pad = " ".repeat(room.saturating_sub(cell_width(&line.text())));
        let fill = line.fill();
        let row = Line::new()
            .push(Tone::Muted, self.upright.as_str())
            .plain(" ".repeat(inset))
            .then(line);
        let row = match fill {
            Some(fill) => row.push_filled(Tone::Plain, fill, pad),
            None => row.plain(pad),
        };
        row.push(Tone::Muted, self.upright.as_str())
    }

    /// The bottom edge, `cells` wide.
    fn bottom(&self, cells: usize) -> Line {
        let [.., left, right] = self.corners;
        let level = self.level.repeat(cells.saturating_sub(2 * UPRIGHT));
        Line::new().push(Tone::Muted, format!("{left}{level}{right}"))
    }
}

mod size;
pub use size::BoxSize;

#[cfg(test)]
mod tests;
