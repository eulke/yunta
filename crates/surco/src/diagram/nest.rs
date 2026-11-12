//! A flowchart with groups, taken apart the way a surface lays it out: the chart
//! outside every group, where each group is one node, and, for each of those
//! nodes, the chart of what the group holds, taken apart the same way. A link that
//! leaves or reaches a node inside a group is a link of the outside chart that
//! leaves or reaches the group, with the node it starts or ends at inside it, so a
//! surface draws it from that node, across the group's edge. A link between two
//! nodes of one group belongs to the chart inside it. Each surface lays every
//! chart out as it lays out one with no group, and draws a group's chart inside
//! the group's box.
//!
//! A link between a group and a node it holds says that a box is a step from or to
//! something inside it, which no drawing can say: such a chart is not taken apart,
//! and each surface says it as it says a chart it cannot draw.

use crate::diagram::{Flowchart, Link};

/// One level of a chart taken apart: the chart at it, and for each of its nodes
/// that stands for a group, the chart inside that group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Nest {
    /// The chart at this level: its nodes, those that stand for a group among
    /// them, and the links between them. It holds no group of its own.
    pub chart: Flowchart,
    /// For each node of [`Nest::chart`], the chart inside it where it stands for
    /// a group.
    pub inner: Vec<Option<Nest>>,
    /// For each link of [`Nest::chart`], the nodes inside a group it starts and
    /// ends at.
    pub ends: Vec<Ends>,
}

/// Where a link starts and ends inside the groups it leaves and reaches: the node
/// it leaves or reaches by its place in that group's chart, none where it leaves
/// or reaches the node itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Ends {
    pub from: Option<usize>,
    pub to: Option<usize>,
}

impl Nest {
    /// `chart` taken apart; `None` where a link joins a group and a node it holds.
    pub(crate) fn of(chart: &Flowchart) -> Option<Nest> {
        let family = Family::of(chart);
        let joins_its_own = chart
            .links
            .iter()
            .any(|link| family.holds(link.from, link.to) || family.holds(link.to, link.from));
        (!joins_its_own).then(|| family.level(chart, None))
    }
}

/// Which group holds each node of a chart, and its place among what that group
/// holds.
struct Family {
    /// The group node holding each node, none for a node outside every group.
    parent: Vec<Option<usize>>,
    /// Each node's place among the nodes held where it is held.
    rank: Vec<usize>,
}

impl Family {
    fn of(chart: &Flowchart) -> Family {
        let mut parent = vec![None; chart.nodes.len()];
        for group in &chart.groups {
            for member in &group.members {
                parent[*member] = Some(group.node);
            }
        }
        let rank = (0..chart.nodes.len())
            .map(|node| {
                (0..node)
                    .filter(|before| parent[*before] == parent[node])
                    .count()
            })
            .collect();
        Family { parent, rank }
    }

    /// Whether the group `group` holds `node`, there or in a group it holds.
    fn holds(&self, group: usize, node: usize) -> bool {
        let mut at = node;
        while let Some(up) = self.parent[at] {
            if up == group {
                return true;
            }
            at = up;
        }
        false
    }

    /// The node held `within` that `node` is, or is held in: none where `node` is
    /// held nowhere inside it.
    fn held_within(&self, node: usize, within: Option<usize>) -> Option<usize> {
        let mut at = node;
        loop {
            if self.parent[at] == within {
                return Some(at);
            }
            at = self.parent[at]?;
        }
    }

    /// Where a link starts or ends inside `outer`, the node at this level it
    /// leaves or reaches: the place, in `outer`'s chart, of what holds `node` there;
    /// none where it is `outer` itself.
    fn inside(&self, outer: usize, node: usize) -> Option<usize> {
        (node != outer)
            .then(|| self.held_within(node, Some(outer)))
            .flatten()
            .map(|held| self.rank[held])
    }

    /// The level of `chart` held `within` a group, or outside every group.
    fn level(&self, chart: &Flowchart, within: Option<usize>) -> Nest {
        let here: Vec<usize> = (0..chart.nodes.len())
            .filter(|node| self.parent[*node] == within)
            .collect();
        let (mut links, mut ends) = (Vec::new(), Vec::new());
        for link in &chart.links {
            let from = self.held_within(link.from, within);
            let to = self.held_within(link.to, within);
            let (Some(from), Some(to)) = (from, to) else {
                continue;
            };
            if from == to {
                continue;
            }
            links.push(Link {
                from: self.rank[from],
                to: self.rank[to],
                ..link.clone()
            });
            ends.push(Ends {
                from: self.inside(from, link.from),
                to: self.inside(to, link.to),
            });
        }
        let groups: Vec<usize> = chart.groups.iter().map(|group| group.node).collect();
        let inner = here
            .iter()
            .map(|node| {
                groups
                    .contains(node)
                    .then(|| self.level(chart, Some(*node)))
            })
            .collect();
        Nest {
            chart: Flowchart {
                direction: chart.direction,
                nodes: here.iter().map(|node| chart.nodes[*node].clone()).collect(),
                links,
                groups: Vec::new(),
            },
            inner,
            ends,
        }
    }
}

#[cfg(test)]
mod tests;
