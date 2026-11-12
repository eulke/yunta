//! What is open over a screen, boxed: the palette, help or a confirmation drawn inside the
//! edges of a box, its title in the top edge, its document drawn by the rule a region is
//! drawn by. The box is as wide as the look and as tall as what it holds, up to the rows it
//! is given; an engine of cells says where it stands. A box takes at least its two sides,
//! four cells: a look narrower than that still draws them, wider than the look.

use super::{Paned, Terminal};
use crate::components::Overlay;
use crate::diagram::{Shape, Stroke};
use crate::glyphs::Glyphs;
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::width::cell_width;

/// The cells between an upright and what the box holds: a line inside never touches the
/// edge it stands in.
const INSIDE: usize = 1;

/// The cells each side of a box takes: its upright, and the cell inside it.
const SIDE: usize = 1 + INSIDE;

/// The rows a box's edges take: the top, which holds its title, and the bottom.
const EDGE_ROWS: usize = 2;

impl Terminal {
    /// `overlay` boxed at this look's width and as tall as what it holds, at most `rows`
    /// high: the title in the top edge, each line inside the uprights, and its document
    /// drawn by the rule a region is drawn by ([`Terminal::pane`]); with what was laid out
    /// of it, and where the cursor stands in the box.
    pub fn boxed(&self, overlay: &Overlay, rows: usize) -> Paned {
        let edges = Edges::of(self.look.glyphs);
        let cells = self.look.width.cells();
        let room = cells.saturating_sub(2 * SIDE);
        let inside = Terminal::on(Look {
            width: Width::exact(room),
            ..self.look
        });
        let held = inside.paned(&overlay.doc, overlay.sight, rows.saturating_sub(EDGE_ROWS));
        let title = Line::new()
            .push(Tone::Strong, overlay.title.as_str())
            .cut(room, self.look.glyphs);
        let mut lines = vec![edges.top(title, cells)];
        lines.extend(held.lines.into_iter().map(|line| edges.inside(line, room)));
        lines.push(edges.bottom(cells));
        lines.truncate(rows);
        Paned {
            lines,
            cursor: held.cursor.map(|(row, cell)| (row + 1, cell + SIDE)),
            laid: held.laid,
        }
    }
}

/// The marks a box is drawn in, in a glyph set: its four corners, the level stroke of its
/// top and bottom, and the upright stroke of its sides.
struct Edges {
    corners: [char; 4],
    level: String,
    upright: String,
}

impl Edges {
    fn of(glyphs: Glyphs) -> Self {
        Edges {
            corners: glyphs.corners(Shape::Box),
            level: glyphs
                .joint(false, false, true, true, Stroke::Open)
                .to_string(),
            upright: glyphs
                .joint(true, true, false, false, Stroke::Open)
                .to_string(),
        }
    }

    /// The top edge, `cells` wide, with `title` in it after its first corner.
    fn top(&self, title: Line, cells: usize) -> Line {
        let [left, right, ..] = self.corners;
        let after = cells.saturating_sub(cell_width(&title.text()) + 2 * SIDE);
        Line::new()
            .push(Tone::Muted, left.to_string())
            .plain(" ")
            .then(title)
            .plain(" ")
            .push(Tone::Muted, format!("{}{right}", self.level.repeat(after)))
    }

    /// `line` inside the uprights, filled out to `room` cells so the far upright stands in
    /// line with the rest.
    fn inside(&self, line: Line, room: usize) -> Line {
        let pad = room.saturating_sub(cell_width(&line.text()));
        Line::new()
            .push(Tone::Muted, self.upright.as_str())
            .plain(" ".repeat(INSIDE))
            .then(line)
            .plain(" ".repeat(pad + INSIDE))
            .push(Tone::Muted, self.upright.as_str())
    }

    /// The bottom edge, `cells` wide.
    fn bottom(&self, cells: usize) -> Line {
        let [.., left, right] = self.corners;
        let level = self.level.repeat(cells.saturating_sub(2));
        Line::new().push(Tone::Muted, format!("{left}{level}{right}"))
    }
}

#[cfg(test)]
mod tests;
