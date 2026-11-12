//! The ribbon on a terminal: each node's name over its stretch, and the plow.

use super::page;
use crate::components::{Ribbon, Stretch};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::cell_width;
use crate::words::Standing;

/// The fewest cells a stretch takes, so a node with a short name still reads
/// as a stretch of furrow beside one with a long name.
const LEAST: usize = 4;

impl Ribbon {
    /// The names over their strokes, each stretch as wide as its name and at
    /// least [`LEAST`] cells, a gap between two. The plow is the running
    /// node's cell where its events put it, drawn strong, and only when the
    /// ribbon has events. Where the names do not fit the line, it is a page's
    /// furrow: the strokes alone, sharing the line.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        if self.stretches.is_empty() {
            return Vec::new();
        }
        let width = look.width.cells();
        let cells: Vec<usize> = self
            .stretches
            .iter()
            .map(|stretch| cell_width(&stretch.name).max(LEAST))
            .collect();
        let needed = cells.iter().sum::<usize>() + cells.len() - 1;
        if needed > width {
            let standings: Vec<Standing> = self.stretches.iter().map(|it| it.standing).collect();
            let apart = page::spaced(standings.len(), width);
            return vec![page::strokes(&standings, width, apart, look)];
        }
        let mut names = Line::new();
        let mut strokes = Line::new();
        let last = self.stretches.len() - 1;
        for (at, (stretch, cells)) in self.stretches.iter().zip(cells).enumerate() {
            if at > 0 {
                names = names.plain(" ");
                strokes = strokes.plain(" ");
            }
            let name = match at == last {
                true => stretch.name.clone(),
                false => format!("{:<cells$}", stretch.name),
            };
            names = names.push(name_tone(stretch.standing), name);
            strokes = self.stroke(strokes, stretch, cells, look);
        }
        vec![names, strokes]
    }

    /// `line` with `stretch`'s stroke, `cells` long: the page's stroke, with
    /// the plow's cell strong on a running node's stretch when it has one.
    fn stroke(&self, line: Line, stretch: &Stretch, cells: usize, look: &Look) -> Line {
        let plow = match (stretch.standing, self.events) {
            (Standing::Running, Some(events)) => Some(cells_into(events, cells)),
            _ => None,
        };
        let Some(at) = plow else {
            return page::stroke(line, stretch.standing, cells, look);
        };
        let line = page::stroke(line, stretch.standing, at, look);
        let glyph = look.glyphs.furrow(crate::glyphs::Plow::Plowed);
        let line = line.push(Tone::Strong, glyph.to_string());
        page::stroke(line, stretch.standing, cells - at - 1, look)
    }
}

/// The cell of a stretch `cells` long where `events` put the plow: one cell
/// further for each, starting over at the stretch's end.
fn cells_into(events: u64, cells: usize) -> usize {
    let cells = u64::try_from(cells).unwrap_or(u64::MAX).max(1);
    usize::try_from(events % cells).unwrap_or(0)
}

/// The tone of a node's name: muted once it finished, faint while it is still
/// ahead or left out, and the tone of its word otherwise, so the running node's
/// name and one that needs a person stand out.
fn name_tone(standing: Standing) -> Tone {
    match standing {
        Standing::Finished => Tone::Muted,
        Standing::NeverRan | Standing::Skipped => Tone::Faint,
        standing => Tone::of(standing.mark()),
    }
}

#[cfg(test)]
mod tests;
