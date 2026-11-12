//! The furrow and the gist, as they hang from a terminal page's headline.

use crate::components::{Furrow, Gist};
use crate::glyphs::Plow;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, wrap_unbroken, INDENT};
use crate::words::Standing;

impl Furrow {
    /// One stroke for each node, as wide as the line shares out, a gap between
    /// two: plowed where the run reached the node, in the tone of its word;
    /// another stroke, faint, where it never ran, and a third, faint, where the
    /// run leaves it out. With more nodes than cells, a cell stands for the
    /// nodes it covers and takes the word that matters most among them. The
    /// strokes are the same with color and without: color repeats what the
    /// words beside them say. A furrow with no node is ground not yet plowed:
    /// the faint line a page about no one run stands under.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let width = look.width.cells();
        if self.0.is_empty() {
            let stroke = look.glyphs.furrow(Plow::Ahead);
            return vec![Line::new().push(Tone::Faint, stroke.to_string().repeat(width))];
        }
        let apart = spaced(self.0.len(), width);
        vec![strokes(&self.0, width, apart, look)]
    }
}

/// The cells a furrow gives each node at most: a stroke and the gap after
/// it, which is when two strokes stand apart.
pub(super) const CELLS_A_NODE: usize = 2;

/// Whether `count` strokes in `cells` cells stand apart: a gap between two
/// strokes when there are two cells a node.
pub(super) fn spaced(count: usize, cells: usize) -> bool {
    cells >= count * CELLS_A_NODE
}

/// Where each of `nodes` stands, as strokes sharing `cells` between them:
/// plowed in the tone of the node's word, ahead and left out faint, with a
/// space between two strokes when `apart`. With more nodes than cells, each
/// cell stands for the nodes it covers, consecutive, and takes the word
/// that matters most among them, so a node that needs a reader is never
/// hidden by where it stands.
pub(super) fn strokes(nodes: &[Standing], cells: usize, apart: bool, look: &Look) -> Line {
    let count = nodes.len();
    let gaps = match apart {
        true => count.saturating_sub(1),
        false => 0,
    };
    let cells = cells.saturating_sub(gaps);
    let mut line = Line::new();
    if count > cells {
        for cell in 0..cells {
            let covered = nodes.get(cell * count / cells..(cell + 1) * count / cells);
            let most = covered
                .unwrap_or_default()
                .iter()
                .copied()
                .max_by_key(|standing| standing.urgency());
            if let Some(standing) = most {
                line = stroke(line, standing, 1, look);
            }
        }
        return line;
    }
    for (at, standing) in nodes.iter().enumerate() {
        let share = |upto: usize| (cells * upto + count / 2) / count;
        line = stroke(line, *standing, share(at + 1) - share(at), look);
        if gaps > 0 && at + 1 < count {
            line = line.plain(" ");
        }
    }
    line
}

/// `line` with the stroke of a node at `standing`, `takes` cells long:
/// plowed in the tone of its word, or faint where it never ran or the run
/// leaves it out.
pub(super) fn stroke(line: Line, standing: Standing, takes: usize, look: &Look) -> Line {
    let (plow, tone) = match standing {
        Standing::Skipped => (Plow::Skipped, Tone::Faint),
        Standing::NeverRan => (Plow::Ahead, Tone::Faint),
        _ => (Plow::Plowed, Tone::of(standing.mark())),
    };
    line.push(tone, look.glyphs.furrow(plow).to_string().repeat(takes))
}

impl Gist {
    /// The sentence wrapped one step in, whole.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let room = look.width.cells().saturating_sub(cell_width(INDENT));
        wrap_unbroken(&self.0, room)
            .into_iter()
            .map(|part| Line::new().plain(INDENT).plain(part))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glyphs::Mark;

    /// With more nodes than cells, each cell takes the most urgent word of
    /// the nodes it covers, so the node that needs a person and the one that
    /// failed are both seen.
    #[test]
    fn a_cell_that_covers_several_nodes_takes_the_most_urgent() {
        let nodes = [
            Standing::Finished,
            Standing::NeedsYou,
            Standing::Finished,
            Standing::Finished,
            Standing::Failed,
            Standing::Finished,
            Standing::NeverRan,
            Standing::NeverRan,
        ];
        let line = strokes(&nodes, 4, false, &Look::plain());
        let tones: Vec<Tone> = line.spans().iter().map(|span| span.tone).collect();
        assert_eq!(
            tones,
            [
                Tone::of(Mark::NeedsYou),
                Tone::of(Mark::Done),
                Tone::of(Mark::Failed),
                Tone::Faint,
            ]
        );
        assert_eq!(cell_width(&line.text()), 4);
    }

    /// With a cell for each node, every node takes its share and none is
    /// merged.
    #[test]
    fn with_cells_enough_every_node_takes_its_own_share() {
        let nodes = [Standing::Finished, Standing::Running, Standing::NeverRan];
        let line = strokes(&nodes, 6, false, &Look::plain());
        let shares: Vec<(Tone, usize)> = line
            .spans()
            .iter()
            .map(|span| (span.tone, cell_width(&span.text)))
            .collect();
        assert_eq!(
            shares,
            [
                (Tone::of(Mark::Done), 2),
                (Tone::of(Mark::Running), 2),
                (Tone::Faint, 2),
            ]
        );
    }
}
