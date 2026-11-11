//! See [`super`]. The links drawn along a layout: one that goes forward through a
//! lane of each layer it crosses, one that closes a cycle around the far side of
//! every box, each with its head and its label, from or to a node inside a group
//! where it leaves or reaches one.

use super::{Layout, Placed, BACK, MARGIN};
use crate::diagram::layer::{Element, Route};
use crate::diagram::nest::Ends;
use crate::diagram::{Flowchart, Stroke};
use crate::surface::html::text::esc;

/// An arrowhead's length: seen at this font's size, and short of any label.
const HEAD: usize = 8;
/// Half an arrowhead's width: half its length reads as a point.
const HEAD_HALF: usize = 4;
/// A link that goes back leaves and enters this part of a box in from its end, so
/// the two ends of a loop on one box never share a point.
const OFF_CENTER: usize = 4;
/// How far over a link its label's baseline stands.
const LIFT: usize = 4;

impl Layout {
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
    /// crosses, its label over its first bend; from or to the node inside a group
    /// `ends` names.
    pub(in crate::surface::html::diagram) fn route(
        &self,
        chart: &Flowchart,
        route: &Route,
        ends: Ends,
    ) -> String {
        let link = &chart.links[route.link];
        let (from, to) = (self.end(link.from, ends.from), self.end(link.to, ends.to));
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
    /// and into the far side of the box it goes back to; from or to the node
    /// inside a group `ends` names.
    pub(in crate::surface::html::diagram) fn back(
        &self,
        chart: &Flowchart,
        at: usize,
        nth: usize,
        ends: Ends,
    ) -> String {
        let link = &chart.links[at];
        let (from, to) = (self.end(link.from, ends.from), self.end(link.to, ends.to));
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

/// The class a link's stroke draws with, when it has one.
fn stroke(stroke: Stroke) -> &'static str {
    match stroke {
        Stroke::Dotted => " class=\"dotted\"",
        Stroke::Thick => " class=\"thick\"",
        Stroke::Arrow | Stroke::Open => "",
    }
}
