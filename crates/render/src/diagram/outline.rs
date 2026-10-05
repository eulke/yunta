//! A flowchart as one line per chain: what a terminal says of a diagram
//! it has no room to draw in boxes, which still reads in the order the
//! chart flows.

use super::{Flowchart, Link};
use crate::Glyphs;

/// Each chain of `chart` as a line — its boxes in the order the chart
/// flows, a label on the link it is written on — then each box no link
/// reaches.
pub(crate) fn outline(chart: &Flowchart, glyphs: Glyphs) -> Vec<String> {
    let incoming = |node: usize| chart.links.iter().filter(|link| link.to == node).count();
    let outgoing = |node: usize| chart.links.iter().filter(|link| link.from == node).count();
    let mut drawn = vec![false; chart.links.len()];
    let mut lines = Vec::new();
    for start in 0..chart.links.len() {
        if drawn[start] {
            continue;
        }
        drawn[start] = true;
        let mut line = format!(
            "{}{}",
            chart.nodes[chart.links[start].from].label,
            step(&chart.links[start], chart, glyphs)
        );
        let mut at = chart.links[start].to;
        // A box one link enters and one leaves goes on the same line.
        while incoming(at) == 1 && outgoing(at) == 1 {
            let Some(next) =
                (0..chart.links.len()).find(|link| !drawn[*link] && chart.links[*link].from == at)
            else {
                break;
            };
            drawn[next] = true;
            line.push_str(&step(&chart.links[next], chart, glyphs));
            at = chart.links[next].to;
        }
        lines.push(line);
    }
    for (at, node) in chart.nodes.iter().enumerate() {
        if incoming(at) == 0 && outgoing(at) == 0 {
            lines.push(node.label.clone());
        }
    }
    lines
}

/// One link, as it reads after the box it leaves: its arrow, its label
/// in parentheses, and the box it reaches.
fn step(link: &Link, chart: &Flowchart, glyphs: Glyphs) -> String {
    let arrow = match link.stroke.points() {
        true => glyphs.arrow(),
        false => glyphs.joins(),
    };
    let label = link
        .label
        .as_ref()
        .map(|label| format!(" ({label})"))
        .unwrap_or_default();
    format!(" {arrow}{label} {}", chart.nodes[link.to].label)
}
