//! A layered chart drawn reading down the page: each layer a row of
//! boxes centered on the widest, each link dropping through the channel
//! below its box on a lane of its own, and each link that goes back
//! climbing a lane at the right of everything.

use super::grid::Grid;
use super::layer::Element;
use super::place::{centered, Laid, GAP};
use crate::Glyphs;

/// Where everything sits on the page.
struct Frame {
    /// Each element's left column, layer by layer.
    xs: Vec<Vec<usize>>,
    /// Each layer's top row and height.
    rows: Vec<(usize, usize)>,
    /// Each channel's top row and height.
    channels: Vec<(usize, usize)>,
    /// The column the first link that goes back climbs.
    lane: usize,
    width: usize,
    height: usize,
}

/// `laid` drawn reading down, in at most `width` cells.
pub(super) fn drawn(laid: &Laid, width: usize, glyphs: Glyphs) -> Option<Grid> {
    let frame = frame(laid);
    if frame.width > width {
        return None;
    }
    let mut grid = Grid::new(frame.width, frame.height);
    for (layer, elements) in laid.layered.layers.iter().enumerate() {
        for (at, element) in elements.iter().enumerate() {
            if let Element::Node(node) = element {
                laid.draw_box(
                    &mut grid,
                    *node,
                    (frame.xs[layer][at], frame.rows[layer].0),
                    glyphs,
                );
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
    Some(grid)
}

/// Where the layers, the channels and the elements in each layer sit.
fn frame(laid: &Laid) -> Frame {
    let layers = &laid.layered.layers;
    let span = |element: &Element| match element {
        Element::Node(node) => laid.sizes[*node].0,
        Element::Through(_) => 1,
    };
    let (xs, inner) = centered(layers, span, GAP);
    let (mut rows, mut channels, mut y) = (Vec::new(), Vec::new(), 0);
    for (at, channel) in laid.lanes.channels.iter().enumerate() {
        let between = at > 0 && at < layers.len();
        let tall = channel.runs + usize::from(channel.label > 0) + usize::from(channel.arrives);
        let tall = if between { tall.max(1) } else { tall };
        channels.push((y, tall));
        y += tall;
        if let Some(layer) = layers.get(at) {
            let high = layer.iter().filter_map(|element| match element {
                Element::Node(node) => Some(laid.sizes[*node].1),
                Element::Through(_) => None,
            });
            let high = high.max().unwrap_or(1);
            rows.push((y, high));
            y += high;
        }
    }
    let lanes = laid.layered.back.len();
    Frame {
        xs,
        rows,
        channels,
        lane: inner + 1,
        width: inner + if lanes > 0 { 2 * lanes } else { 0 },
        height: y,
    }
}

impl Frame {
    /// The column a link leaves or reaches `element` at, its place `at`
    /// in `layer`.
    fn middle(&self, laid: &Laid, layer: usize, at: usize, element: Element) -> usize {
        match element {
            Element::Node(node) => self.xs[layer][at] + laid.sizes[node].0 / 2,
            Element::Through(_) => self.xs[layer][at],
        }
    }

    /// The last row of `element` in `layer`, which a link leaves it from.
    fn bottom(&self, laid: &Laid, layer: usize, element: Element) -> usize {
        let (top, high) = self.rows[layer];
        match element {
            Element::Node(node) => top + laid.sizes[node].1 - 1,
            Element::Through(_) => top + high - 1,
        }
    }

    /// The row a label sits on in `channel`, and the row its heads point
    /// down from.
    fn ends(&self, laid: &Laid, channel: usize) -> (usize, usize) {
        let (top, tall) = self.channels[channel];
        (top + laid.lanes.channels[channel].runs, top + tall - 1)
    }
}

/// One forward route, step by step through the channels it crosses.
fn forward(
    laid: &Laid,
    frame: &Frame,
    grid: &mut Grid,
    (route, lanes): (&super::layer::Route, &Vec<usize>),
    glyphs: Glyphs,
) -> Option<()> {
    let stroke = laid.stroke(route.link);
    let first = laid.layered.layer_of(laid.chart.links[route.link].from);
    for (step, pair) in route.path.windows(2).enumerate() {
        let (from, to) = (first + step, first + step + 1);
        let place = |layer: usize, element: Element| {
            let at = laid.layered.layers[layer]
                .iter()
                .position(|e| *e == element);
            frame.middle(laid, layer, at.unwrap_or_default(), element)
        };
        let (sx, tx) = (place(from, pair[0]), place(to, pair[1]));
        let run = frame.channels[to].0 + lanes[step];
        grid.vertical(sx, frame.bottom(laid, from, pair[0]), run, stroke);
        grid.horizontal(run, sx, tx, stroke);
        let (label_row, head_row) = frame.ends(laid, to);
        match pair[1] {
            Element::Through(_) => {
                let (top, high) = frame.rows[to];
                grid.vertical(tx, run, top + high - 1, stroke);
            }
            Element::Node(_) => {
                grid.vertical(tx, run, head_row, stroke);
                if stroke.points() {
                    grid.put(tx, head_row, glyphs.head(true));
                }
                labelled(grid, laid.label(route.link), (tx, label_row))?;
            }
        }
    }
    Some(())
}

/// One link that goes back: down into the channel below its box, right
/// to its lane, up to the channel above the box it reaches, and down
/// onto it.
fn back(
    laid: &Laid,
    frame: &Frame,
    grid: &mut Grid,
    (link, (leaving, arriving), lane): (usize, (usize, usize), usize),
    glyphs: Glyphs,
) -> Option<()> {
    let stroke = laid.stroke(link);
    let (from, to) = (laid.chart.links[link].from, laid.chart.links[link].to);
    let (low, high) = (laid.layered.layer_of(from), laid.layered.layer_of(to));
    let place = |layer: usize, node: usize| {
        let element = Element::Node(node);
        let at = laid.layered.layers[layer]
            .iter()
            .position(|e| *e == element);
        frame.middle(laid, layer, at.unwrap_or_default(), element)
    };
    let (sx, tx) = (place(low, from), place(high, to));
    let out = frame.channels[low + 1].0 + leaving;
    let into = frame.channels[high].0 + arriving;
    grid.vertical(
        sx,
        frame.bottom(laid, low, Element::Node(from)),
        out,
        stroke,
    );
    grid.horizontal(out, sx, lane, stroke);
    grid.vertical(lane, into, out, stroke);
    grid.horizontal(into, tx, lane, stroke);
    let (label_row, head_row) = frame.ends(laid, high);
    grid.vertical(tx, into, head_row, stroke);
    if stroke.points() {
        grid.put(tx, head_row, glyphs.head(true));
    }
    labelled(grid, laid.label(link), (tx, label_row))
}

/// `label` on its row beside the head at column `x`: to its right, or
/// to its left when the right is taken; `None` when neither has room.
fn labelled(grid: &mut Grid, label: Option<&str>, (x, row): (usize, usize)) -> Option<()> {
    let Some(label) = label else {
        return Some(());
    };
    let wide = label.chars().count();
    let left = x.checked_sub(wide + 1);
    let at = [Some(x + 2), left]
        .into_iter()
        .flatten()
        .find(|at| grid.room(*at, row, label))?;
    grid.write(at, row, label);
    Some(())
}
