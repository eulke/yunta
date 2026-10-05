//! A flowchart's boxes in layers: each one layer past the furthest box
//! that links to it, and each layer in the order that crosses the fewest
//! links it can find. A link that spans layers passes through each one
//! between, so it never runs through a box; a link that closes a cycle
//! goes back along the side, so it never pushes a box below itself.

use super::Flowchart;

/// What sits in a layer: a box, or a link passing through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Element {
    /// A box, by its place in the chart.
    Node(usize),
    /// The link at this place in the chart, passing through this layer.
    Through(usize),
}

/// A link that goes forward: the elements it runs through, from its box
/// to the box it reaches, one to a layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Route {
    pub link: usize,
    pub path: Vec<Element>,
}

/// The boxes of a chart by layer, the links that go forward, and those
/// that close a cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Layered {
    /// Each layer's elements, in the order they are drawn.
    pub layers: Vec<Vec<Element>>,
    pub routes: Vec<Route>,
    /// The links, by their places in the chart, that close a cycle.
    pub back: Vec<usize>,
}

impl Layered {
    /// The layer `node` is in.
    pub(crate) fn layer_of(&self, node: usize) -> usize {
        self.layers
            .iter()
            .position(|layer| layer.contains(&Element::Node(node)))
            .unwrap_or_default()
    }
}

/// `chart` in layers.
pub(crate) fn layered(chart: &Flowchart) -> Layered {
    let closing = backward(chart);
    let forward: Vec<usize> = (0..chart.links.len()).filter(|at| !closing[*at]).collect();
    let depth = depths(chart, &forward);
    let count = depth.iter().max().map_or(0, |deepest| deepest + 1);
    let mut layers: Vec<Vec<Element>> = vec![Vec::new(); count];
    for (node, layer) in depth.iter().enumerate() {
        layers[*layer].push(Element::Node(node));
    }
    let routes: Vec<Route> = forward
        .iter()
        .map(|link| {
            let (from, to) = (chart.links[*link].from, chart.links[*link].to);
            let mut path = vec![Element::Node(from)];
            for between in layers.iter_mut().take(depth[to]).skip(depth[from] + 1) {
                between.push(Element::Through(*link));
                path.push(Element::Through(*link));
            }
            path.push(Element::Node(to));
            Route { link: *link, path }
        })
        .collect();
    let steps: Vec<(Element, Element)> = routes
        .iter()
        .flat_map(|route| route.path.windows(2).map(|pair| (pair[0], pair[1])))
        .collect();
    // One sweep down and one up per layer is as settled as an order by
    // averages gets.
    for _ in 0..count {
        for layer in 1..count {
            reorder(&mut layers, layer, layer - 1, &steps, false);
        }
        for layer in (0..count.saturating_sub(1)).rev() {
            reorder(&mut layers, layer, layer + 1, &steps, true);
        }
    }
    Layered {
        layers,
        routes,
        back: (0..chart.links.len()).filter(|at| closing[*at]).collect(),
    }
}

/// Each box's layer: one past the deepest box a forward link reaches it
/// from.
fn depths(chart: &Flowchart, forward: &[usize]) -> Vec<usize> {
    let mut depth = vec![0usize; chart.nodes.len()];
    // A longest path settles once no link pushes its box further: at
    // most once per box.
    for _ in 0..chart.nodes.len() {
        let mut moved = false;
        for link in forward {
            let (from, to) = (chart.links[*link].from, chart.links[*link].to);
            if depth[to] < depth[from] + 1 {
                depth[to] = depth[from] + 1;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    depth
}

/// Which links close a cycle: those that reach a box still being walked
/// from, walking from each box in the order the chart names them.
fn backward(chart: &Flowchart) -> Vec<bool> {
    #[derive(Clone, Copy, PartialEq)]
    enum Walk {
        Unseen,
        Open,
        Done,
    }
    let mut walk = vec![Walk::Unseen; chart.nodes.len()];
    let mut closing = vec![false; chart.links.len()];
    for start in 0..chart.nodes.len() {
        if walk[start] != Walk::Unseen {
            continue;
        }
        // Each frame: a box, and where to look for its next link.
        let mut stack = vec![(start, 0usize)];
        walk[start] = Walk::Open;
        while let Some(&(node, next)) = stack.last() {
            let found = (next..chart.links.len()).find(|at| chart.links[*at].from == node);
            let Some(at) = found else {
                walk[node] = Walk::Done;
                stack.pop();
                continue;
            };
            if let Some(frame) = stack.last_mut() {
                frame.1 = at + 1;
            }
            let to = chart.links[at].to;
            match walk[to] {
                Walk::Open => closing[at] = true,
                Walk::Unseen => {
                    walk[to] = Walk::Open;
                    stack.push((to, 0));
                }
                Walk::Done => {}
            }
        }
    }
    closing
}

/// `layers[layer]` ordered by the average place of what it links with in
/// `layers[by]`: the layer after it when `after`, the one before
/// otherwise. An element linked with nothing there keeps its place.
fn reorder(
    layers: &mut [Vec<Element>],
    layer: usize,
    by: usize,
    steps: &[(Element, Element)],
    after: bool,
) {
    let place = |element: Element| {
        layers[by]
            .iter()
            .position(|candidate| *candidate == element)
    };
    let mut weighed: Vec<(Element, f64)> = layers[layer]
        .iter()
        .enumerate()
        .map(|(at, element)| {
            let linked: Vec<usize> = steps
                .iter()
                .filter_map(|(from, to)| match after {
                    true if from == element => place(*to),
                    false if to == element => place(*from),
                    _ => None,
                })
                .collect();
            let weight = match linked.is_empty() {
                true => at as f64,
                false => linked.iter().sum::<usize>() as f64 / linked.len() as f64,
            };
            (*element, weight)
        })
        .collect();
    weighed.sort_by(|a, b| a.1.total_cmp(&b.1));
    layers[layer] = weighed.into_iter().map(|(element, _)| element).collect();
}
