//! A diagram an author wrote, in boxes when it fits the line.

use crate::components::Code;
use crate::diagram::{Diagram, Flowchart};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{wrap, INDENT};

impl Diagram {
    /// A chart this reads in boxes, the way its author wrote it when it
    /// fits the line, turned when only that fits, and one line per chain
    /// when neither does; any other diagram as its source, saying it is
    /// not drawn here. A chart that reads up or left is drawn reading
    /// down or right: the same boxes and links, the other way up.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let Some(chart) = &self.chart else {
            return Code::whole(
                "diagram",
                Some(
                    "a diagram this terminal does not draw; it is drawn where Markdown is read"
                        .to_string(),
                ),
                &self.source,
            )
            .lines(look);
        };
        let room = look.width.cells().saturating_sub(INDENT.len());
        let across = chart.direction.across();
        let boxed = crate::diagram::place::drawn(chart, across, room, look.glyphs)
            .or_else(|| crate::diagram::place::drawn(chart, !across, room, look.glyphs));
        match boxed {
            Some(rows) => rows
                .into_iter()
                .map(|row| Line::new().plain(INDENT).plain(row))
                .collect(),
            None => chains(chart, look),
        }
    }
}

/// `chart` as one line per chain, each wrapped under itself.
fn chains(chart: &Flowchart, look: &Look) -> Vec<Line> {
    let under = format!("{INDENT}  ");
    let room = look.width.cells().saturating_sub(under.len());
    crate::diagram::outline::outline(chart, look.glyphs)
        .iter()
        .flat_map(|chain| {
            wrap(chain, room)
                .into_iter()
                .enumerate()
                .map(|(at, part)| match at {
                    0 => Line::new().plain(INDENT).push(Tone::Strong, part),
                    _ => Line::new().plain(under.as_str()).push(Tone::Strong, part),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}
