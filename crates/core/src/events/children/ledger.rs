//! The runs this run started, folded once.
//!
//! Five places used to walk the child pair with their own rule: one to
//! find the open child under a node, one to close it, one to aggregate
//! its spend, two to draw it. Every surface that asks what a run bore
//! reads it here.

use crate::events::children::kinds::ChildEvent;
use crate::events::meta::EventMeta;
use crate::events::{TerminalState, TokenUsage};
use crate::hash::ContentHash;
use crate::ids::{NodeId, RunId};

/// One child run, as the pair of kinds records it: born under a node,
/// and closed or still open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildLink {
    pub run_id: RunId,
    /// The parent's `kind: workflow` node that bore it; `None` for a
    /// link the log recorded under no node.
    pub node: Option<NodeId>,
    /// The child's frozen workflow, never its name: a caller that labels
    /// a child reads the child's own manifest.
    pub workflow_hash: ContentHash,
    /// How the child closed, or `None` while it is still open.
    pub terminal: Option<TerminalState>,
}

/// Every child this run bore, in the order it bore them, and what the
/// loop that bore them reported.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChildLedger {
    links: Vec<ChildLink>,
    /// What every closed child spent. A child's spend always aggregates
    /// into its parent, so a promotion chain's every member counts
    /// exactly once.
    tokens: TokenUsage,
    /// The highest iteration any loop node reported.
    iterations: u32,
}

impl ChildLedger {
    /// Every child link, oldest first.
    pub fn links(&self) -> &[ChildLink] {
        &self.links
    }

    /// The child `node` bore and has not closed, if there is one.
    pub fn open_under(&self, node: &NodeId) -> Option<&ChildLink> {
        self.links
            .iter()
            .rev()
            .find(|link| link.node.as_ref() == Some(node) && link.terminal.is_none())
    }

    /// What every closed child spent.
    pub fn tokens(&self) -> TokenUsage {
        self.tokens
    }

    /// The highest loop iteration the log reports.
    pub fn iterations(&self) -> u32 {
        self.iterations
    }

    /// Folds one child-domain event.
    pub fn apply(&mut self, event: &ChildEvent, meta: &EventMeta<'_>) {
        match event {
            ChildEvent::Created(p) => self.links.push(ChildLink {
                run_id: p.child_run_id.clone(),
                node: meta.node.cloned(),
                workflow_hash: p.child_workflow_hash.clone(),
                terminal: None,
            }),
            ChildEvent::Finished(p) => {
                self.tokens += p.tokens;
                match self
                    .links
                    .iter_mut()
                    .rev()
                    .find(|link| link.run_id == p.child_run_id)
                {
                    Some(link) => link.terminal = Some(p.terminal_state),
                    // A log truncated, or written by an engine that did
                    // not record births, can carry a close with no birth
                    // behind it. The link exists, already closed, rather
                    // than the child going unreported.
                    None => self.links.push(ChildLink {
                        run_id: p.child_run_id.clone(),
                        node: meta.node.cloned(),
                        workflow_hash: p.child_workflow_hash.clone(),
                        terminal: Some(p.terminal_state),
                    }),
                }
            }
            ChildEvent::LoopIteration(p) => self.iterations = self.iterations.max(p.iteration),
        }
    }
}
