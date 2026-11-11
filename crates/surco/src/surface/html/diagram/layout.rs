//! See [`super`]. Where everything of one level of a chart stands in the drawing:
//! each box, a group's among them, in its layer, and each lane a link takes
//! through a layer. The links drawn along it are `links`'.

mod links;

use std::collections::BTreeMap;

use crate::diagram::layer::{Element, Layered};
use crate::diagram::{Direction, Flowchart, Node, Shape};
use crate::surface::html::text::esc;
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
/// Around the drawing, so a stroke at its edge is whole.
const MARGIN: usize = 2;
/// Between a group's edge and what it holds.
const GROUP_PAD: usize = 10;
/// A group's title, over what it holds: one line of a label's font.
const TITLE: usize = LINE;

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

/// Where everything of one level of a chart stands.
pub(super) struct Layout {
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
    /// Where everything inside each group box stands, in its own drawing.
    inside: Vec<Option<Layout>>,
    long: usize,
    wide: usize,
}

impl Layout {
    /// Where everything of `chart` stands, in the order its direction lays it out:
    /// a box sized for its label, or, where `groups` gives the size of what a group
    /// holds drawn, for that and the group's title.
    pub(super) fn of(
        chart: &Flowchart,
        layered: &Layered,
        groups: &[Option<(usize, usize)>],
    ) -> Self {
        let horizontal = matches!(chart.direction, Direction::Right | Direction::Left);
        let onward = matches!(chart.direction, Direction::Right | Direction::Down);
        let sizes: Vec<(usize, usize)> = chart
            .nodes
            .iter()
            .zip(groups)
            .map(|(node, held)| match held {
                Some(held) => group_size(node, *held, horizontal),
                None => size(node, horizontal),
            })
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
            inside: Vec::new(),
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

    /// What each group box holds, laid out in its own drawing.
    pub(super) fn hold(&mut self, inside: Vec<Option<Layout>>) {
        self.inside = inside;
    }

    /// Where the drawing of what group box `at` holds starts, along the flow and
    /// across it: past the group's edge, and past its title.
    fn origin(&self, at: usize) -> (usize, usize) {
        let place = self.boxes[at];
        match self.horizontal {
            true => (place.along + GROUP_PAD, place.across + GROUP_PAD + TITLE),
            false => (place.along + GROUP_PAD + TITLE, place.across + GROUP_PAD),
        }
    }

    /// The box a link leaves or reaches at node `at`: its own, or that of `member`
    /// inside the group `at` is, where it stands in this drawing.
    fn end(&self, at: usize, member: Option<usize>) -> Placed {
        let inside = member.zip(self.inside.get(at).and_then(Option::as_ref));
        let Some((member, inside)) = inside else {
            return self.boxes[at];
        };
        let (along, across) = self.origin(at);
        let held = inside.boxes[member];
        Placed {
            along: along + held.along,
            across: across + held.across,
            ..held
        }
    }

    /// Group box `at`: its edge, its title, and `held`, the drawing of what it
    /// holds, inside it.
    pub(super) fn group(&self, at: usize, node: &Node, held: &str) -> String {
        let place = self.boxes[at];
        let (x, y, width, height) = match self.horizontal {
            true => (place.along, place.across, place.long, place.wide),
            false => (place.across, place.along, place.wide, place.long),
        };
        let (along, across) = self.origin(at);
        let (left, top) = match self.horizontal {
            true => (along, across),
            false => (across, along),
        };
        format!(
            "<rect class=\"group\" x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" rx=\"{SQUARE}\"/><text class=\"title\" x=\"{}\" y=\"{}\">{}</text><g transform=\"translate({left} {top})\">{held}</g>",
            x + GROUP_PAD,
            y + GROUP_PAD + BASELINE,
            esc(&node.label)
        )
    }

    /// The drawing's width and height, with room for its links that go back.
    pub(super) fn size(&self, back: usize) -> (usize, usize) {
        let wide = self.wide + BACK * back + 2 * MARGIN;
        match self.horizontal {
            true => (self.long, wide),
            false => (wide, self.long),
        }
    }

    /// One box and its label.
    pub(super) fn node(&self, at: usize, node: &Node) -> String {
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
}

/// Where across group box `node` the node `member` inside it stands, from -½ to
/// ½ of the group's width across the flow, what it holds drawn `held` big as
/// `inside` lays it out: what a link from or to that node weighs when the
/// layers are ordered.
pub(super) fn side_of(node: &Node, held: (usize, usize), inside: &Layout, member: usize) -> f64 {
    let horizontal = inside.horizontal;
    let (_, wide) = group_size(node, held, horizontal);
    let inset = match horizontal {
        true => GROUP_PAD + TITLE,
        false => GROUP_PAD,
    };
    (inset + inside.boxes[member].middle()) as f64 / wide as f64 - 0.5
}

/// How much of the flow and of its width group box `node` takes: the drawing of
/// what it holds, `held` wide and high, inside its edge and under its title, or
/// as wide as its title where that is wider.
fn group_size(node: &Node, (wide, high): (usize, usize), horizontal: bool) -> (usize, usize) {
    let titled = cell_width(&node.label) * CELL;
    let width = wide.max(titled) + 2 * GROUP_PAD;
    let height = high + 2 * GROUP_PAD + TITLE;
    match horizontal {
        true => (width, height),
        false => (height, width),
    }
}

/// A box, its corners curved by `curve`.
fn rect(x: usize, y: usize, width: usize, height: usize, curve: usize) -> String {
    format!("<rect x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" rx=\"{curve}\"/>")
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
