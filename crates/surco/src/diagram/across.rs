//! A layered chart drawn reading across the page: each layer a column of
//! boxes centered on the tallest, each link crossing the channel after
//! its box on a lane of its own with its label on the stretch into the
//! box it reaches, and each link that goes back along a lane below
//! everything. A link that leaves or reaches a node inside a group runs
//! from or onto it, across the group's edge, where nothing inside the
//! group stands in its way.

use crate::diagram::grid::Grid;
use crate::diagram::layer::Element;
use crate::diagram::place::{centered, Laid, Placed};
use crate::glyphs::Glyphs;

/// Where everything sits on the page.
struct Frame {
    /// Each element's top row, layer by layer.
    ys: Vec<Vec<usize>>,
    /// Each layer's left column and width.
    columns: Vec<(usize, usize)>,
    /// Each channel's left column and width.
    channels: Vec<(usize, usize)>,
    /// The row the first link that goes back runs along.
    lane: usize,
    width: usize,
    height: usize,
}

/// `laid` drawn reading across, in at most `width` cells.
pub(super) fn drawn(laid: &Laid, width: usize, glyphs: Glyphs) -> Option<Placed> {
    let frame = frame(laid);
    if frame.width > width {
        return None;
    }
    let mut grid = Grid::new(frame.width, frame.height);
    let mut boxes = vec![(0, 0); laid.sizes.len()];
    for (layer, elements) in laid.layered.layers.iter().enumerate() {
        for (at, element) in elements.iter().enumerate() {
            if let Element::Node(node) = element {
                let corner = (frame.columns[layer].0, frame.ys[layer][at]);
                laid.draw_box(&mut grid, *node, corner, glyphs);
                boxes[*node] = corner;
            }
        }
    }
    for (route, lanes) in laid.layered.routes.iter().zip(&laid.lanes.steps) {
        forward(laid, &frame, &mut grid, (route, lanes), glyphs)?;
    }
    for (at, (link, lanes)) in laid.layered.back.iter().zip(&laid.lanes.back).enumerate() {
        back(
            laid,
            &frame,
            &mut grid,
            (*link, *lanes, frame.lane + 2 * at),
            glyphs,
        )?;
    }
    Some(Placed {
        grid,
        boxes,
        sizes: laid.sizes.clone(),
    })
}

/// Where the layers, the channels and the elements in each layer sit.
fn frame(laid: &Laid) -> Frame {
    let layers = &laid.layered.layers;
    let span = |element: &Element| match element {
        Element::Node(node) => laid.sizes[*node].1,
        Element::Through(_) => 1,
    };
    let (ys, inner) = centered(layers, span, 1);
    let (mut columns, mut channels, mut x) = (Vec::new(), Vec::new(), 0);
    for (at, channel) in laid.lanes.channels.iter().enumerate() {
        let between = at > 0 && at < layers.len();
        let label = match channel.label {
            0 => 0,
            wide => wide + 2,
        };
        // A cell after the boxes before the first lane, so a lane never
        // reads as the edge of the box beside it.
        let stub = usize::from(channel.runs > 0);
        let wide = stub + channel.runs + label + usize::from(channel.arrives);
        let wide = if between { wide.max(2) } else { wide };
        channels.push((x, wide));
        x += wide;
        if let Some(layer) = layers.get(at) {
            let widest = layer.iter().filter_map(|element| match element {
                Element::Node(node) => Some(laid.sizes[*node].0),
                Element::Through(_) => None,
            });
            let widest = widest.max().unwrap_or(1);
            columns.push((x, widest));
            x += widest;
        }
    }
    let lanes = laid.layered.back.len();
    Frame {
        ys,
        columns,
        channels,
        lane: inner + 1,
        width: x,
        height: inner + if lanes > 0 { 2 * lanes } else { 0 },
    }
}

impl Frame {
    /// The row a link leaves or reaches `element` at, its place `at` in
    /// `layer`.
    fn middle(&self, laid: &Laid, layer: usize, at: usize, element: Element) -> usize {
        match element {
            Element::Node(node) => self.ys[layer][at] + laid.sizes[node].1 / 2,
            Element::Through(_) => self.ys[layer][at],
        }
    }

    /// The last column of `element` in `layer`, which a link leaves it
    /// from.
    fn right(&self, laid: &Laid, layer: usize, element: Element) -> usize {
        let (left, wide) = self.columns[layer];
        match element {
            Element::Node(node) => left + laid.sizes[node].0 - 1,
            Element::Through(_) => left + wide - 1,
        }
    }

    /// The column of `lane` in `channel`, past the cell that keeps the
    /// lanes off the boxes before them.
    fn lane(&self, channel: usize, lane: usize) -> usize {
        self.channels[channel].0 + 1 + lane
    }

    /// The column a label in `channel` starts at: past every lane.
    fn label(&self, laid: &Laid, channel: usize) -> usize {
        self.lane(channel, laid.lanes.channels[channel].runs)
    }

    /// The column the heads in `channel` point right from.
    fn head(&self, channel: usize) -> usize {
        let (left, wide) = self.channels[channel];
        left + wide - 1
    }

    /// The top-left cell of box `node`, in `layer`.
    fn corner(&self, laid: &Laid, layer: usize, node: usize) -> (usize, usize) {
        let at = laid.layered.layers[layer]
            .iter()
            .position(|e| *e == Element::Node(node));
        (
            self.columns[layer].0,
            self.ys[layer][at.unwrap_or_default()],
        )
    }

    /// Where a link leaves box `node`, in `layer`: the row it runs along and the
    /// column it starts from, the box's middle and its last column; or, from
    /// `member` inside the group `node` is, that member's, where nothing inside
    /// stands between it and the group's right edge.
    fn leaving(
        &self,
        (laid, grid): (&Laid, &Grid),
        (layer, node): (usize, usize),
        member: Option<usize>,
    ) -> Option<(usize, usize)> {
        let (x, y) = self.corner(laid, layer, node);
        let (wide, high) = laid.sizes[node];
        let Some(((mx, my), (mw, mh))) = member.and_then(|m| laid.member(node, m)) else {
            return Some((y + high / 2, x + wide - 1));
        };
        let (row, right) = (y + my + mh / 2, x + mx + mw - 1);
        let after = (right + 1..x + wide - 1).map(|column| (column, row));
        grid.clear(after).then_some((row, right))
    }

    /// Where a link reaches box `node`, in `layer`: the row it runs along and the
    /// column its head points from, the box's middle on `head`, the last column
    /// of the channel before it; or, at `member` inside the group `node` is, the
    /// column before that member, where nothing inside stands between it and the
    /// group's left edge.
    fn reaching(
        &self,
        (laid, grid): (&Laid, &Grid),
        (layer, node): (usize, usize),
        (member, head): (Option<usize>, usize),
    ) -> Option<(usize, usize)> {
        let (x, y) = self.corner(laid, layer, node);
        let Some(((mx, my), (_, mh))) = member.and_then(|m| laid.member(node, m)) else {
            return Some((y + laid.sizes[node].1 / 2, head));
        };
        let (row, before) = (y + my + mh / 2, x + mx - 1);
        let between = (x + 1..=before).map(|column| (column, row));
        grid.clear(between).then_some((row, before))
    }
}

/// One forward route, step by step through the channels it crosses.
fn forward(
    laid: &Laid,
    frame: &Frame,
    grid: &mut Grid,
    (route, lanes): (&crate::diagram::layer::Route, &Vec<usize>),
    glyphs: Glyphs,
) -> Option<()> {
    let stroke = laid.stroke(route.link);
    let ends = laid.ends[route.link];
    let first = laid.layered.layer_of(laid.chart.links[route.link].from);
    for (step, pair) in route.path.windows(2).enumerate() {
        let (from, to) = (first + step, first + step + 1);
        let place = |layer: usize, element: Element| {
            let at = laid.layered.layers[layer]
                .iter()
                .position(|e| *e == element);
            frame.middle(laid, layer, at.unwrap_or_default(), element)
        };
        let (sy, start) = match pair[0] {
            Element::Node(node) => frame.leaving((laid, grid), (from, node), ends.from)?,
            through => (place(from, through), frame.right(laid, from, through)),
        };
        let run = frame.lane(to, lanes[step]);
        let edge = frame.head(to);
        let (ty, head) = match pair[1] {
            Element::Node(node) => frame.reaching((laid, grid), (to, node), (ends.to, edge))?,
            through => (place(to, through), edge),
        };
        grid.horizontal(sy, start, run, stroke);
        grid.vertical(run, sy, ty, stroke);
        match pair[1] {
            Element::Through(_) => {
                let (left, wide) = frame.columns[to];
                grid.horizontal(ty, run, left + wide - 1, stroke);
            }
            Element::Node(_) => {
                grid.horizontal(ty, run, head, stroke);
                if stroke.points() {
                    grid.put(head, ty, glyphs.head(false));
                }
                let begin = frame.label(laid, to);
                labelled(grid, laid.label(route.link), (begin, edge, ty))?;
            }
        }
    }
    Some(())
}

/// One link that goes back: right into the channel after its box, down
/// to its lane, left to the channel before the box it reaches, and up
/// and across onto it.
fn back(
    laid: &Laid,
    frame: &Frame,
    grid: &mut Grid,
    (link, (leaving, arriving), lane): (usize, (usize, usize), usize),
    glyphs: Glyphs,
) -> Option<()> {
    let stroke = laid.stroke(link);
    let ends = laid.ends[link];
    let (from, to) = (laid.chart.links[link].from, laid.chart.links[link].to);
    let (late, early) = (laid.layered.layer_of(from), laid.layered.layer_of(to));
    let edge = frame.head(early);
    let (sy, start) = frame.leaving((laid, grid), (late, from), ends.from)?;
    let (ty, head) = frame.reaching((laid, grid), (early, to), (ends.to, edge))?;
    let out = frame.lane(late + 1, leaving);
    let into = frame.lane(early, arriving);
    grid.horizontal(sy, start, out, stroke);
    grid.vertical(out, sy, lane, stroke);
    grid.horizontal(lane, into, out, stroke);
    grid.vertical(into, ty, lane, stroke);
    grid.horizontal(ty, into, head, stroke);
    if stroke.points() {
        grid.put(head, ty, glyphs.head(false));
    }
    let begin = frame.label(laid, early);
    labelled(grid, laid.label(link), (begin, edge, ty))
}

/// `label` written over row `row` from `start`, past every lane of its
/// channel and short of the head at `head`, a space either side; `None`
/// when that stretch holds anything but lines.
fn labelled(
    grid: &mut Grid,
    label: Option<&str>,
    (start, head, row): (usize, usize, usize),
) -> Option<()> {
    let Some(label) = label else {
        return Some(());
    };
    let said = format!(" {label} ");
    let at = start;
    if at + said.chars().count() > head || !grid.lines_only(at, row, &said) {
        return None;
    }
    grid.write(at, row, &said);
    Some(())
}
