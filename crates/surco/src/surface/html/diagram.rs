//! A diagram as a page draws it: its boxes in the layers the chart falls into,
//! each link along the route that keeps it off every box, and every word as
//! text. A group is a box around the drawing of what it holds, titled with its
//! label, each level of the chart drawn as the one with no group is
//! ([`Nest`]); a link that leaves or reaches a node inside a group runs from or
//! to that node. A source this cannot read stands as written. Where everything
//! stands is `layout`'s.

mod layout;

use super::text::esc;
use crate::diagram::layer::{layered, Sides};
use crate::diagram::nest::Nest;
use crate::diagram::{Diagram, Flowchart};
use layout::Layout;

/// A diagram: its drawing when it reads as a chart with a box in it, and its
/// source as written otherwise.
pub(super) fn diagram(diagram: &Diagram) -> String {
    let drawable = diagram
        .chart
        .as_ref()
        .filter(|chart| !chart.nodes.is_empty())
        .and_then(|chart| Some((chart, Nest::of(chart)?)));
    let Some((chart, nest)) = drawable else {
        return format!("<pre class=\"lines\">{}</pre>", esc(&diagram.source));
    };
    let drawing = Drawing::of(&nest);
    let (width, height) = drawing.size;
    format!(
        "<figure class=\"scroll\"><svg class=\"flow\" viewBox=\"0 0 {width} {height}\" width=\"{width}\" height=\"{height}\" role=\"img\" aria-label=\"{}\">{}</svg></figure>",
        esc(&said(chart)),
        drawing.svg
    )
}

/// One level of a chart drawn: its boxes, its groups with what each holds drawn
/// inside, and its links; where everything of it stands; and how big it is.
struct Drawing {
    svg: String,
    layout: Layout,
    size: (usize, usize),
}

impl Drawing {
    /// `nest`'s level drawn, each group's chart drawn first.
    fn of(nest: &Nest) -> Drawing {
        let chart = &nest.chart;
        let inner: Vec<Option<Drawing>> = nest
            .inner
            .iter()
            .map(|inner| inner.as_ref().map(Drawing::of))
            .collect();
        let groups: Vec<Option<(usize, usize)>> = inner
            .iter()
            .map(|drawn| drawn.as_ref().map(|drawn| drawn.size))
            .collect();
        let side = |node: usize, member: Option<usize>| match (member, &inner[node]) {
            (Some(member), Some(held)) => {
                layout::side_of(&chart.nodes[node], held.size, &held.layout, member)
            }
            _ => 0.0,
        };
        let sides: Sides = chart
            .links
            .iter()
            .zip(&nest.ends)
            .map(|(link, ends)| (side(link.from, ends.from), side(link.to, ends.to)))
            .collect();
        let layers = layered(chart, &sides);
        let mut layout = Layout::of(chart, &layers, &groups);
        let mut svg = String::new();
        let mut inside = Vec::new();
        for (at, (node, held)) in chart.nodes.iter().zip(inner).enumerate() {
            match held {
                Some(held) => {
                    svg.push_str(&layout.group(at, node, &held.svg));
                    inside.push(Some(held.layout));
                }
                None => {
                    svg.push_str(&layout.node(at, node));
                    inside.push(None);
                }
            }
        }
        layout.hold(inside);
        for route in &layers.routes {
            svg.push_str(&layout.route(chart, route, nest.ends[route.link]));
        }
        for (nth, link) in layers.back.iter().enumerate() {
            svg.push_str(&layout.back(chart, *link, nth, nest.ends[*link]));
        }
        let size = layout.size(layers.back.len());
        Drawing { svg, layout, size }
    }
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
