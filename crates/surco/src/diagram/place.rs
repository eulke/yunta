//! A layered flowchart placed on a page: the boxes in their layers, and
//! between each two layers a channel where every link that crosses it
//! runs on a lane of its own, its label beside the head it ends in.
//!
//! A chart with groups is placed a level at a time ([`Nest`]): each group's
//! chart is drawn first, and the group is a box of lines, titled with its
//! label on its top edge, that holds that drawing; outside, the group is laid
//! out as any box is. A link that leaves or reaches a node inside a group runs
//! from or to that node, across the group's edge.

use crate::diagram::grid::Grid;
use crate::diagram::layer::{layered, Element, Layered, Sides};
use crate::diagram::nest::{Ends, Nest};
use crate::diagram::{Flowchart, Shape, Stroke};
use crate::glyphs::Glyphs;
use crate::width::{cell_width, wrap};

/// The widest a line of a box's label runs before it wraps, and the
/// widest label a link can carry and still be drawn.
pub(super) const LABEL_CELLS: usize = 24;

/// The cells between two boxes side by side, which is room enough for a
/// link to pass between them and still read as apart from both.
pub(super) const GAP: usize = 4;

/// Between a group's edge and what it holds, on every side: its edge, then a
/// cell of room a link crossing the edge turns or points in.
const GROUP_INSET: usize = 2;

/// What a group's top edge takes around its title: its corner and a line before
/// it, a space either side of it, and a line and its corner after.
const GROUP_TITLE: usize = 6;

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
    let nest = Nest::of(chart)?;
    let placed = placed(&nest, across, width, glyphs)?;
    Some(placed.grid.rows(glyphs))
}

/// A chart drawn: its page, and where each of its boxes stands on it and how big
/// it is.
pub(super) struct Placed {
    pub grid: Grid,
    /// Each box's top-left cell.
    pub boxes: Vec<(usize, usize)>,
    /// Each box's width and height, borders included.
    pub sizes: Vec<(usize, usize)>,
}

/// One level of `nest` drawn, each group's chart drawn first and held in its box;
/// `None` where any of it does not fit `width` or a link finds no way.
fn placed(nest: &Nest, across: bool, width: usize, glyphs: Glyphs) -> Option<Placed> {
    let interiors = nest
        .inner
        .iter()
        .map(|inner| match inner {
            Some(inner) => placed(inner, across, width, glyphs).map(Some),
            None => Some(None),
        })
        .collect::<Option<Vec<_>>>()?;
    let laid = Laid::of(nest, interiors, across);
    match across {
        false => crate::diagram::down::drawn(&laid, width, glyphs),
        true => crate::diagram::across::drawn(&laid, width, glyphs),
    }
}

/// A chart ready to place: its layers, each box's label in lines and its
/// size, what each group holds drawn, where each link starts and ends inside
/// a group, and what each channel between two layers carries.
pub(super) struct Laid<'a> {
    pub chart: &'a Flowchart,
    pub layered: Layered,
    /// Each box's label, a line to a row of it.
    pub labels: Vec<Vec<String>>,
    /// Each box's width and height, borders included.
    pub sizes: Vec<(usize, usize)>,
    /// What each box that stands for a group holds, drawn.
    pub interiors: Vec<Option<Placed>>,
    /// Where each link starts and ends inside the groups it leaves and reaches.
    pub ends: &'a [Ends],
    pub lanes: Lanes,
}

impl<'a> Laid<'a> {
    fn of(nest: &'a Nest, interiors: Vec<Option<Placed>>, across: bool) -> Self {
        let chart = &nest.chart;
        let labels: Vec<Vec<String>> = chart
            .nodes
            .iter()
            .map(|node| wrap(&node.label, LABEL_CELLS))
            .collect();
        let sizes = chart
            .nodes
            .iter()
            .zip(&labels)
            .zip(&interiors)
            .map(|((node, lines), interior)| match interior {
                Some(interior) => {
                    let (wide, high) = interior.grid.size();
                    let titled = cell_width(&node.label) + GROUP_TITLE;
                    ((wide + 2 * GROUP_INSET).max(titled), high + 2 * GROUP_INSET)
                }
                None => {
                    let widest = lines.iter().map(|line| cell_width(line)).max();
                    (widest.unwrap_or(0) + 4, lines.len() + 2)
                }
            })
            .collect();
        let mut laid = Laid {
            chart,
            layered: Layered::default(),
            labels,
            sizes,
            interiors,
            ends: &nest.ends,
            lanes: Lanes::default(),
        };
        laid.layered = layered(chart, &laid.sides(across));
        laid.lanes = lanes(chart, &laid.layered);
        laid
    }

    /// Where along its boxes' sides each link leaves and reaches them: the
    /// middle, or where the node inside a group it leaves or reaches stands
    /// along the group's side, across the flow.
    fn sides(&self, across: bool) -> Sides {
        let side = |node: usize, member: Option<usize>| {
            let Some(((x, y), (wide, high))) = member.and_then(|m| self.member(node, m)) else {
                return 0.0;
            };
            let (group_wide, group_high) = self.sizes[node];
            match across {
                false => (x + wide / 2) as f64 / group_wide as f64 - 0.5,
                true => (y + high / 2) as f64 / group_high as f64 - 0.5,
            }
        };
        self.chart
            .links
            .iter()
            .zip(self.ends)
            .map(|(link, ends)| (side(link.from, ends.from), side(link.to, ends.to)))
            .collect()
    }

    /// The box of `member` inside the group box `node`: its top-left cell, from
    /// the group's, and its width and height. `None` where `node` holds nothing.
    pub fn member(&self, node: usize, member: usize) -> Option<((usize, usize), (usize, usize))> {
        let interior = self.interiors[node].as_ref()?;
        let (x, y) = self.inset(node, interior);
        let (mx, my) = *interior.boxes.get(member)?;
        Some(((x + mx, y + my), *interior.sizes.get(member)?))
    }

    /// Where a group's drawing stands inside its box `node`: past its edge and
    /// its room, centered across a box its title makes wider.
    fn inset(&self, node: usize, interior: &Placed) -> (usize, usize) {
        let (wide, _) = interior.grid.size();
        let spare = self.sizes[node].0 - wide - 2 * GROUP_INSET;
        (GROUP_INSET + spare / 2, GROUP_INSET)
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
    /// and its label inside; or, for a group, its edges, its title on the top
    /// one, and what it holds inside.
    pub fn draw_box(&self, grid: &mut Grid, node: usize, (x, y): (usize, usize), glyphs: Glyphs) {
        if let Some(interior) = &self.interiors[node] {
            self.draw_group(grid, node, (x, y), interior);
            return;
        }
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

impl Laid<'_> {
    /// The group box `node` drawn at `x`, `y`: its edges in lines, which a link
    /// crossing them joins, its label over the top one, and what it holds.
    fn draw_group(&self, grid: &mut Grid, node: usize, (x, y): (usize, usize), interior: &Placed) {
        let (width, height) = self.sizes[node];
        let (right, bottom) = (x + width - 1, y + height - 1);
        grid.horizontal(y, x, right, Stroke::Arrow);
        grid.horizontal(bottom, x, right, Stroke::Arrow);
        grid.vertical(x, y, bottom, Stroke::Arrow);
        grid.vertical(right, y, bottom, Stroke::Arrow);
        grid.write(x + 2, y, &format!(" {} ", self.chart.nodes[node].label));
        let (inset_x, inset_y) = self.inset(node, interior);
        grid.inlaid((x + inset_x, y + inset_y), &interior.grid);
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
#[derive(Default)]
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
