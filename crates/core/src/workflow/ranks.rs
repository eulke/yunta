//! What runs from a node on, by rank: what a decision says follows each answer.

use std::collections::{BTreeMap, BTreeSet};

use super::optional::LeftOut;
use super::Workflow;
use crate::ids::NodeId;

impl Workflow {
    /// What runs from `starts` on, by rank: `starts`, then what waits on them, and
    /// so on, each node a rank past the furthest of what it waits on among these,
    /// in the order the workflow declares them. What the run left out is no rank,
    /// and it leads to nothing.
    pub fn ranks_from(&self, starts: &[NodeId], left_out: &[LeftOut]) -> Vec<Vec<NodeId>> {
        let gone: BTreeSet<&NodeId> = left_out.iter().map(|left| &left.node).collect();
        let mut held: BTreeSet<&NodeId> =
            starts.iter().filter(|node| !gone.contains(node)).collect();
        // `nodes` is declared with what a node waits on before it only by convention, so
        // walk until nothing is added.
        loop {
            let before = held.len();
            for node in &self.nodes {
                if !gone.contains(&node.id) && node.depends_on.iter().any(|dep| held.contains(dep))
                {
                    held.insert(&node.id);
                }
            }
            if held.len() == before {
                break;
            }
        }
        self.ranked(&held)
    }

    /// What lies on a way from `from` to `to`, by rank, `to` last. What leads
    /// elsewhere is not on the way. Nothing where no way leads there.
    pub fn ranks_between(
        &self,
        from: &NodeId,
        to: &NodeId,
        left_out: &[LeftOut],
    ) -> Vec<Vec<NodeId>> {
        let down: BTreeSet<NodeId> = self
            .ranks_from(std::slice::from_ref(from), left_out)
            .into_iter()
            .flatten()
            .collect();
        if !down.contains(to) {
            return Vec::new();
        }
        let mut held: BTreeSet<&NodeId> = BTreeSet::from([to]);
        loop {
            let before = held.len();
            for node in &self.nodes {
                if held.contains(&node.id) {
                    held.extend(node.depends_on.iter().filter(|dep| down.contains(*dep)));
                }
            }
            if held.len() == before {
                break;
            }
        }
        self.ranked(&held)
    }

    /// `held` by rank, each in the order the workflow declares them.
    fn ranked(&self, held: &BTreeSet<&NodeId>) -> Vec<Vec<NodeId>> {
        let mut rank: BTreeMap<&NodeId, usize> = BTreeMap::new();
        // As many passes as nodes settle every rank, whatever order they are declared in.
        for _ in 0..self.nodes.len() {
            for node in self.nodes.iter().filter(|node| held.contains(&node.id)) {
                let furthest = node
                    .depends_on
                    .iter()
                    .filter_map(|dep| rank.get(dep))
                    .max()
                    .map_or(0, |at| at + 1);
                rank.insert(&node.id, furthest);
            }
        }
        let deepest = rank.values().max().map_or(0, |at| at + 1);
        let mut ranks = vec![Vec::new(); deepest];
        for node in &self.nodes {
            if let Some(ranked) = rank.get(&node.id).and_then(|at| ranks.get_mut(*at)) {
                ranked.push(node.id.clone());
            }
        }
        ranks
    }
}
