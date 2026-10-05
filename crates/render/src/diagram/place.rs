//! A layered flowchart placed on a page: the boxes in their layers, and
//! between each two layers a channel where every link that crosses it
//! runs on a lane of its own, its label beside the head it ends in.

use super::grid::Grid;
use super::layer::{layered, Element, Layered};
use super::{Flowchart, Shape, Stroke};
use crate::{cell_width, wrap, Glyphs};

/// The widest a line of a box's label runs before it wraps, and the
/// widest label a link can carry and still be drawn.
pub(super) const LABEL_CELLS: usize = 24;

/// The cells between two boxes side by side, which is room enough for a
/// link to pass between them and still read as apart from both.
pub(super) const GAP: usize = 4;

/// `chart` drawn reading down the page, or across it when `across`, in
/// at most `width` cells; `None` when it does not fit, or when a link's
/// label finds no room beside its head.
pub(crate) fn drawn(
    chart: &Flowchart,
    across: bool,
    width: usize,
    glyphs: Glyphs,
) -> Option<Vec<String>> {
    let long = chart.links.iter().filter_map(|link| link.label.as_ref());
    if long
        .into_iter()
        .any(|label| cell_width(label) > LABEL_CELLS)
    {
        return None;
    }
    let laid = Laid::of(chart);
    let grid = match across {
        false => super::down::drawn(&laid, width, glyphs)?,
        true => super::across::drawn(&laid, width, glyphs)?,
    };
    Some(grid.rows(glyphs))
}

/// A chart ready to place: its layers, each box's label in lines and its
/// size, and what each channel between two layers carries.
pub(super) struct Laid<'a> {
    pub chart: &'a Flowchart,
    pub layered: Layered,
    /// Each box's label, a line to a row of it.
    pub labels: Vec<Vec<String>>,
    /// Each box's width and height, borders included.
    pub sizes: Vec<(usize, usize)>,
    pub lanes: Lanes,
}

impl<'a> Laid<'a> {
    fn of(chart: &'a Flowchart) -> Self {
        let layered = layered(chart);
        let labels: Vec<Vec<String>> = chart
            .nodes
            .iter()
            .map(|node| wrap(&node.label, LABEL_CELLS))
            .collect();
        let sizes = labels
            .iter()
            .map(|lines| {
                let widest = lines.iter().map(|line| cell_width(line)).max();
                (widest.unwrap_or(0) + 4, lines.len() + 2)
            })
            .collect();
        let lanes = lanes(chart, &layered);
        Laid {
            chart,
            layered,
            labels,
            sizes,
            lanes,
        }
    }

    /// How the link at `link` is drawn.
    pub fn stroke(&self, link: usize) -> Stroke {
        self.chart.links[link].stroke
    }

    /// The label of the link at `link`, when it has one.
    pub fn label(&self, link: usize) -> Option<&str> {
        self.chart.links[link].label.as_deref()
    }

    /// Box `node` drawn at `x`, `y`: its corners by its shape, its edges,
    /// and its label inside.
    pub fn draw_box(&self, grid: &mut Grid, node: usize, (x, y): (usize, usize), glyphs: Glyphs) {
        let (width, height) = self.sizes[node];
        let shape: Shape = self.chart.nodes[node].shape;
        let [top_left, top_right, bottom_left, bottom_right] = glyphs.corners(shape);
        let level = glyphs.joint(false, false, true, true, Stroke::Arrow);
        let upright = glyphs.joint(true, true, false, false, Stroke::Arrow);
        let edge: String = std::iter::repeat_n(level, width - 2).collect();
        grid.write(x, y, &format!("{top_left}{edge}{top_right}"));
        grid.write(
            x,
            y + height - 1,
            &format!("{bottom_left}{edge}{bottom_right}"),
        );
        for row in 1..height - 1 {
            grid.put(x, y + row, upright);
            grid.put(x + width - 1, y + row, upright);
            let line = self.labels[node].get(row - 1).map_or("", String::as_str);
            grid.write(x + 2, y + row, line);
        }
    }
}

/// Where each element of each layer starts along it, each layer centered
/// on the longest, and how long that one is: `span` is how much of the
/// layer an element takes, `gap` what lies between two.
pub(super) fn centered(
    layers: &[Vec<Element>],
    span: impl Fn(&Element) -> usize,
    gap: usize,
) -> (Vec<Vec<usize>>, usize) {
    let lengths: Vec<usize> = layers
        .iter()
        .map(|layer| layer.iter().map(&span).sum::<usize>() + gap * layer.len().saturating_sub(1))
        .collect();
    let longest = lengths.iter().copied().max().unwrap_or(0);
    let starts = layers
        .iter()
        .zip(&lengths)
        .map(|(layer, length)| {
            let mut at = (longest - length) / 2;
            layer
                .iter()
                .map(|element| {
                    let start = at;
                    at += span(element) + gap;
                    start
                })
                .collect()
        })
        .collect();
    (starts, longest)
}

/// What crosses one channel between two layers.
#[derive(Debug, Clone, Default)]
pub(super) struct Channel {
    /// How many lanes its links run on.
    pub runs: usize,
    /// Whether a link ends in it on a box, and so needs room for its head.
    pub arrives: bool,
    /// The widest label of a link that ends in it.
    pub label: usize,
}

/// The lane each link takes in each channel it crosses.
pub(super) struct Lanes {
    /// One per gap between layers, and one before the first and after
    /// the last for the links that go back.
    pub channels: Vec<Channel>,
    /// For each forward route, the lane each of its steps takes, in the
    /// channel after the layer it leaves.
    pub steps: Vec<Vec<usize>>,
    /// For each link that goes back, the lanes it takes leaving its box
    /// and arriving at the one it reaches.
    pub back: Vec<(usize, usize)>,
}

/// The lanes of `layered`'s links.
fn lanes(chart: &Flowchart, layered: &Layered) -> Lanes {
    let mut channels = vec![Channel::default(); layered.layers.len() + 1];
    let mut take = |channel: usize, ends: Option<usize>| {
        let lane = channels[channel].runs;
        channels[channel].runs += 1;
        if let Some(link) = ends {
            channels[channel].arrives = true;
            let label = chart.links[link].label.as_deref().map_or(0, cell_width);
            channels[channel].label = channels[channel].label.max(label);
        }
        lane
    };
    let steps = layered
        .routes
        .iter()
        .map(|route| {
            let first = layered.layer_of(chart.links[route.link].from);
            let last = route.path.len() - 2;
            (0..=last)
                .map(|step| take(first + step + 1, (step == last).then_some(route.link)))
                .collect()
        })
        .collect();
    let back = layered
        .back
        .iter()
        .map(|link| {
            let (from, to) = (chart.links[*link].from, chart.links[*link].to);
            let leaving = take(layered.layer_of(from) + 1, None);
            let arriving = take(layered.layer_of(to), Some(*link));
            (leaving, arriving)
        })
        .collect();
    Lanes {
        channels,
        steps,
        back,
    }
}
