//! A diagram as a page draws it: its boxes in the layers the chart falls into,
//! each link along the route that keeps it off every box, and every word as
//! text. A source this cannot read stands as written.

use std::collections::BTreeMap;

use super::text::esc;
use crate::diagram::layer::{layered, Element, Layered, Route};
use crate::diagram::{Diagram, Direction, Flowchart, Node, Shape, Stroke};
use crate::width::cell_width;

/// A cell of a label's font, in the drawing's units: 12.5 px letters are about
/// 7 wide, and this leaves room for a label of wide letters.
const CELL: usize = 9;
/// A line of a label: its font's height and the space under it.
const LINE: usize = 17;
/// Where a line's baseline sits under its top, so its letters stand centered.
const BASELINE: usize = 12;
/// Between a label and its box's edge, along the line.
const PAD_ALONG: usize = 12;
/// Between a label and its box's edge, across the line.
const PAD_ACROSS: usize = 7;
/// What a diamond adds to its box, along and across: its corners cut into the
/// room a label would have.
const DIAMOND: (usize, usize) = (28, 18);
/// The curve of a box that says it is round: half the height of one line's box.
const ROUND: usize = 14;
/// The curve of any other box: enough to read as drawn, not as a table's cell.
const SQUARE: usize = 5;
/// Between two layers: room for a link to bend and for a label over it.
const LAYER_GAP: usize = 96;
/// Between two things in a layer.
const STACK_GAP: usize = 18;
/// What a link passing through a layer takes in it: a lane of its own.
const LANE: usize = 16;
/// Room past the boxes for each link that goes back.
const BACK: usize = 30;
/// An arrowhead's length: seen at this font's size, and short of any label.
const HEAD: usize = 8;
/// Half an arrowhead's width: half its length reads as a point.
const HEAD_HALF: usize = 4;
/// A link that goes back leaves and enters this part of a box in from its end, so
/// the two ends of a loop on one box never share a point.
const OFF_CENTER: usize = 4;
/// How far over a link its label's baseline stands.
const LIFT: usize = 4;
/// Around the drawing, so a stroke at its edge is whole.
const MARGIN: usize = 2;

/// A diagram: its drawing when it reads as a chart with a box in it, and its
/// source as written otherwise.
pub(super) fn diagram(diagram: &Diagram) -> String {
    let Some(chart) = diagram
        .chart
        .as_ref()
        .filter(|chart| !chart.nodes.is_empty())
    else {
        return format!("<pre class=\"lines\">{}</pre>", esc(&diagram.source));
    };
    let layers = layered(chart);
    let layout = Layout::of(chart, &layers);
    let mut drawn = String::new();
    for (at, node) in chart.nodes.iter().enumerate() {
        drawn.push_str(&layout.node(at, node));
    }
    for route in &layers.routes {
        drawn.push_str(&layout.route(chart, route));
    }
    for (nth, link) in layers.back.iter().enumerate() {
        drawn.push_str(&layout.back(chart, *link, nth));
    }
    let (width, height) = layout.size(layers.back.len());
    format!(
        "<figure class=\"scroll\"><svg class=\"flow\" viewBox=\"0 0 {width} {height}\" width=\"{width}\" height=\"{height}\" role=\"img\" aria-label=\"{}\">{drawn}</svg></figure>",
        esc(&said(chart))
    )
}

/// What the drawing says to a reader who cannot see it: each link in words, with
/// what it is labelled, then each box no link touches.
fn said(chart: &Flowchart) -> String {
    let name = |at: usize| chart.nodes[at].label.replace('\n', " ");
    let links = chart.links.iter().map(|link| match &link.label {
        Some(label) => format!("{} leads to {} ({label})", name(link.from), name(link.to)),
        None => format!("{} leads to {}", name(link.from), name(link.to)),
    });
    let alone = (0..chart.nodes.len())
        .filter(|at| {
            !chart
                .links
                .iter()
                .any(|link| link.from == *at || link.to == *at)
        })
        .map(name);
    links.chain(alone).collect::<Vec<_>>().join(", ")
}

/// A box, in the drawing's own axes: where it starts along the flow and across
/// it, and how much it takes of each.
#[derive(Debug, Clone, Copy, Default)]
struct Placed {
    along: usize,
    across: usize,
    long: usize,
    wide: usize,
}

impl Placed {
    /// The middle of its side that faces across the flow.
    fn middle(self) -> usize {
        self.across + self.wide / 2
    }
}

/// How much of the flow and of its width a box takes, for its label.
fn size(node: &Node, horizontal: bool) -> (usize, usize) {
    let lines: Vec<&str> = node.label.split('\n').collect();
    let widest = lines
        .iter()
        .map(|line| cell_width(line))
        .max()
        .unwrap_or_default();
    let mut width = widest * CELL + 2 * PAD_ALONG;
    let mut height = lines.len() * LINE + 2 * PAD_ACROSS;
    if matches!(node.shape, Shape::Decision) {
        width += DIAMOND.0;
        height += DIAMOND.1;
    }
    match horizontal {
        true => (width, height),
        false => (height, width),
    }
}

/// Where everything of a chart stands.
struct Layout {
    /// Whether the flow runs along x: otherwise along y.
    horizontal: bool,
    /// Whether a later layer stands further along: otherwise nearer the start.
    onward: bool,
    boxes: Vec<Placed>,
    /// The layer each box is in.
    layer_of: Vec<usize>,
    /// Where each layer starts along the flow, and how long it is.
    layers: Vec<(usize, usize)>,
    /// The lane a link has in a layer it passes through, by layer and link.
    lanes: BTreeMap<(usize, usize), usize>,
    long: usize,
    wide: usize,
}

impl Layout {
    /// Where everything of `chart` stands, in the order its direction lays it out.
    fn of(chart: &Flowchart, layered: &Layered) -> Self {
        let horizontal = matches!(chart.direction, Direction::Right | Direction::Left);
        let onward = matches!(chart.direction, Direction::Right | Direction::Down);
        let sizes: Vec<(usize, usize)> = chart
            .nodes
            .iter()
            .map(|node| size(node, horizontal))
            .collect();
        let stacks: Vec<usize> = layered
            .layers
            .iter()
            .map(|layer| stack(layer, &sizes))
            .collect();
        let wide = stacks.iter().copied().max().unwrap_or_default();
        let (layers, long) = lengths(layered, &sizes, onward);
        let mut layout = Layout {
            horizontal,
            onward,
            boxes: vec![Placed::default(); chart.nodes.len()],
            layer_of: vec![0; chart.nodes.len()],
            layers,
            lanes: BTreeMap::new(),
            long,
            wide,
        };
        for (at, layer) in layered.layers.iter().enumerate() {
            let cross = MARGIN + (wide - stacks[at]) / 2;
            layout.place(at, layer, cross, &sizes);
        }
        layout
    }

    /// Stacks one layer's elements from `cross` on.
    fn place(&mut self, at: usize, layer: &[Element], mut cross: usize, sizes: &[(usize, usize)]) {
        let (start, length) = self.layers[at];
        for element in layer {
            match element {
                Element::Node(node) => {
                    let (long, wide) = sizes[*node];
                    let along = start + (length - long) / 2;
                    self.boxes[*node] = Placed {
                        along,
                        across: cross,
                        long,
                        wide,
                    };
                    self.layer_of[*node] = at;
                    cross += wide + STACK_GAP;
                }
                Element::Through(link) => {
                    self.lanes.insert((at, *link), cross + LANE / 2);
                    cross += LANE + STACK_GAP;
                }
            }
        }
    }

    /// The drawing's width and height, with room for its links that go back.
    fn size(&self, back: usize) -> (usize, usize) {
        let wide = self.wide + BACK * back + 2 * MARGIN;
        match self.horizontal {
            true => (self.long, wide),
            false => (wide, self.long),
        }
    }

    /// The two coordinates of a point, written for a path.
    fn point(&self, along: usize, across: usize) -> String {
        match self.horizontal {
            true => format!("{along} {across}"),
            false => format!("{across} {along}"),
        }
    }

    /// A curve from one point to another, bending halfway along the flow.
    fn curve(&self, from: (usize, usize), to: (usize, usize)) -> String {
        let bend = (from.0 + to.0) / 2;
        format!(
            " C {} {} {}",
            self.point(bend, from.1),
            self.point(bend, to.1),
            self.point(to.0, to.1)
        )
    }

    /// One box and its label.
    fn node(&self, at: usize, node: &Node) -> String {
        let place = self.boxes[at];
        let (x, y, width, height) = match self.horizontal {
            true => (place.along, place.across, place.long, place.wide),
            false => (place.across, place.along, place.wide, place.long),
        };
        let shape = match node.shape {
            Shape::Decision => format!(
                "<polygon class=\"node\" points=\"{},{y} {},{} {},{} {x},{}\"/>",
                x + width / 2,
                x + width,
                y + height / 2,
                x + width / 2,
                y + height,
                y + height / 2,
            ),
            Shape::Round => rect(x, y, width, height, ROUND),
            Shape::Box => rect(x, y, width, height, SQUARE),
        };
        let lines: Vec<&str> = node.label.split('\n').collect();
        let top = y + (height - lines.len() * LINE) / 2;
        let words: String = lines
            .iter()
            .enumerate()
            .map(|(row, line)| {
                format!(
                    "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\">{}</text>",
                    x + width / 2,
                    top + row * LINE + BASELINE,
                    esc(line)
                )
            })
            .collect();
        format!("{shape}{words}")
    }

    /// Where a link leaves a box along the flow.
    fn leaving(&self, place: Placed) -> usize {
        match self.onward {
            true => place.along + place.long,
            false => place.along,
        }
    }

    /// Where a link reaches a box along the flow.
    fn reaching(&self, place: Placed) -> usize {
        match self.onward {
            true => place.along,
            false => place.along + place.long,
        }
    }

    /// A link that goes forward: to its box through a lane of each layer it
    /// crosses, its label over its first bend.
    fn route(&self, chart: &Flowchart, route: &Route) -> String {
        let link = &chart.links[route.link];
        let (from, to) = (self.boxes[link.from], self.boxes[link.to]);
        let start = (self.leaving(from), from.middle());
        let mut at = start;
        let mut path = format!("M {}", self.point(at.0, at.1));
        let mut first = None;
        let crossed = route
            .path
            .iter()
            .filter(|element| matches!(element, Element::Through(_)))
            .count();
        for step in 0..crossed {
            let layer = self.layer_of[link.from] + 1 + step;
            let (begin, length) = self.layers[layer];
            let lane = self
                .lanes
                .get(&(layer, route.link))
                .copied()
                .unwrap_or(at.1);
            let (enter, leave) = match self.onward {
                true => (begin, begin + length),
                false => (begin + length, begin),
            };
            path.push_str(&self.curve(at, (enter, lane)));
            first.get_or_insert((enter, lane));
            path.push_str(&format!(" L {}", self.point(leave, lane)));
            at = (leave, lane);
        }
        let tip = (self.reaching(to), to.middle());
        let arrow = !matches!(link.stroke, Stroke::Open);
        let end = match (arrow, self.onward) {
            (true, true) => tip.0 - HEAD,
            (true, false) => tip.0 + HEAD,
            (false, _) => tip.0,
        };
        path.push_str(&self.curve(at, (end, tip.1)));
        let mut out = format!("<path{} d=\"{path}\"/>", stroke(link.stroke));
        if arrow {
            out.push_str(&self.head(tip, end));
        }
        if let Some(label) = &link.label {
            let over = first.unwrap_or((end, tip.1));
            out.push_str(&self.label(label, (start.0 + over.0) / 2, (start.1 + over.1) / 2));
        }
        out
    }

    /// The head of a link that reaches `tip` along the flow, from `base`.
    fn head(&self, tip: (usize, usize), base: usize) -> String {
        format!(
            "<polygon class=\"head\" points=\"{} {} {}\"/>",
            self.point(tip.0, tip.1).replace(' ', ","),
            self.point(base, tip.1 - HEAD_HALF).replace(' ', ","),
            self.point(base, tip.1 + HEAD_HALF).replace(' ', ","),
        )
    }

    /// A link's label, just over the point it stands at.
    fn label(&self, label: &str, along: usize, across: usize) -> String {
        let (x, y) = match self.horizontal {
            true => (along, across - LIFT),
            false => (across, along - LIFT),
        };
        format!(
            "<text class=\"label\" x=\"{x}\" y=\"{y}\" text-anchor=\"middle\">{}</text>",
            esc(label)
        )
    }

    /// A link that closes a cycle: out of its box's far side, past every box,
    /// and into the far side of the box it goes back to.
    fn back(&self, chart: &Flowchart, at: usize, nth: usize) -> String {
        let link = &chart.links[at];
        let (from, to) = (self.boxes[link.from], self.boxes[link.to]);
        let past = MARGIN + self.wide + BACK * (nth + 1);
        let start = (
            from.along + from.long - from.long / OFF_CENTER,
            from.across + from.wide,
        );
        let tip = (to.along + to.long / OFF_CENTER, to.across + to.wide);
        let arrow = !matches!(link.stroke, Stroke::Open);
        let end = tip.1 + if arrow { HEAD } else { 0 };
        let mut out = format!(
            "<path{} d=\"M {} C {} {} {}\"/>",
            stroke(link.stroke),
            self.point(start.0, start.1),
            self.point(start.0, past),
            self.point(tip.0, past),
            self.point(tip.0, end),
        );
        if arrow {
            out.push_str(&format!(
                "<polygon class=\"head\" points=\"{} {} {}\"/>",
                self.point(tip.0, tip.1).replace(' ', ","),
                self.point(tip.0 - HEAD_HALF, end).replace(' ', ","),
                self.point(tip.0 + HEAD_HALF, end).replace(' ', ","),
            ));
        }
        if let Some(label) = &link.label {
            // A cubic curve's middle: an eighth of each end and three of each bend.
            let across = (start.1 + end + 6 * past) / 8;
            out.push_str(&self.label(label, (start.0 + tip.0) / 2, across));
        }
        out
    }
}

/// A box, its corners curved by `curve`.
fn rect(x: usize, y: usize, width: usize, height: usize, curve: usize) -> String {
    format!("<rect x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" rx=\"{curve}\"/>")
}

/// The class a link's stroke draws with, when it has one.
fn stroke(stroke: Stroke) -> &'static str {
    match stroke {
        Stroke::Dotted => " class=\"dotted\"",
        Stroke::Thick => " class=\"thick\"",
        Stroke::Arrow | Stroke::Open => "",
    }
}

/// How much of the flow's width a layer takes, its elements stacked.
fn stack(layer: &[Element], sizes: &[(usize, usize)]) -> usize {
    let taken: usize = layer
        .iter()
        .map(|element| match element {
            Element::Node(node) => sizes[*node].1,
            Element::Through(_) => LANE,
        })
        .sum();
    taken + STACK_GAP * layer.len().saturating_sub(1)
}

/// Where each layer starts along the flow and how long it is, in the order the
/// direction lays them out, and the length of the whole.
fn lengths(
    layered: &Layered,
    sizes: &[(usize, usize)],
    onward: bool,
) -> (Vec<(usize, usize)>, usize) {
    let count = layered.layers.len();
    let mut layers = vec![(0, 0); count];
    let mut along = MARGIN;
    let order: Vec<usize> = match onward {
        true => (0..count).collect(),
        false => (0..count).rev().collect(),
    };
    for at in order {
        let length = layered.layers[at]
            .iter()
            .filter_map(|element| match element {
                Element::Node(node) => Some(sizes[*node].0),
                Element::Through(_) => None,
            })
            .max()
            .unwrap_or_default();
        layers[at] = (along, length);
        along += length + LAYER_GAP;
    }
    (layers, along.saturating_sub(LAYER_GAP) + MARGIN)
}
