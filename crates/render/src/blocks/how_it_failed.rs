//! How a node failed, as the pane of a run that ended failed says it at the node its end
//! names, and as a failure's card in place rests on it: the end of what it printed, quoted as
//! every surface quotes a failure ([`FailureDetail`]), with no word of where the rest is,
//! which the node's printed page keeps, a check whose command regressed quoting what that
//! command printed too; then what the node is, the attempt that failed and how long it
//! worked, labelled in the column a node's own page sets its labels in. A check's kind names
//! its builtin beside it, as its pane's title does.
//!
//! Over a card short of rows it says itself in fewer ([`HowItFailed::abridged`]): the attempt
//! gives way first; then what the command printed gives way from its top, the lines it cuts
//! counted, while it keeps its two newest lines over what the node is and how long it worked;
//! then those give way too, and what it printed keeps down to its newest line, which says
//! what failed last.

use surco::components::{Evidence, Fields};
use surco::doc::Block;
use surco::figures::duration;
use yunta_core::events::{Failure, RedCheck};
use yunta_core::{Node, NodeKind};
use yunta_replay::frame::NodeFrame;

use super::how_it_stands::TOOK;
use super::FailureDetail;
use crate::watched::time_taken;

/// The label of what a node is.
const KIND: &str = "kind";

/// The label of the attempt a node stands at.
const ATTEMPT: &str = "attempt";

/// The newest lines of what a command printed a card's grounds keep over what the node is,
/// beside the row that counts the lines cut above them, as a feed keeps its two newest rows.
const KEPT_OVER_ITS_FACTS: usize = 2;

/// How a node failed: the failure, the node's frame and its declaration, where the run's
/// workflow holds it. Not [`FailureDetail`], the failure's own words alone, nor a failure's
/// card, which asks what to do about it.
pub struct HowItFailed<'a> {
    pub failure: &'a Failure,
    pub node: &'a NodeFrame,
    pub declared: Option<&'a Node>,
}

impl HowItFailed<'_> {
    /// Its blocks: the end of what it printed, where it printed anything, then what the node
    /// is, its attempt and how long it worked.
    pub fn blocks(&self) -> Vec<Block> {
        let said = match self.printed(None) {
            Some(printed) => vec![printed],
            None => FailureDetail {
                failure: self.failure,
                whole: None,
            }
            .blocks(),
        };
        said.into_iter().chain([self.facts(true).into()]).collect()
    }

    /// Its blocks said again in fewer rows, each in fewer than the one before: its attempt
    /// left out; then what the command printed cut from its top, the lines cut counted, down
    /// to its two newest lines, what the node is and how long it worked under it; then what it
    /// printed alone, down to its newest line. None where it quotes no command's output, which
    /// gives way whole.
    pub fn abridged(&self) -> Vec<Vec<Block>> {
        let Some(Block::Evidence(whole)) = self.printed(None) else {
            return Vec::new();
        };
        // The count takes a row of its own, so cutting one line saves none.
        let most = whole.quoted().len().saturating_sub(2);
        let cut = |kept: Option<usize>| self.printed(kept).into_iter();
        let with_its_facts = std::iter::once(None)
            .chain((KEPT_OVER_ITS_FACTS..=most).rev().map(Some))
            .map(|kept| cut(kept).chain([self.facts(false).into()]).collect());
        let alone = (1..=most).rev().map(|kept| cut(Some(kept)).collect());
        with_its_facts.chain(alone).collect()
    }

    /// The end of what its command printed, quoted, as many of its last lines as `kept` says
    /// where a pane cut it from its top: where it exited printing something, or where a
    /// check's command regressed. None for any other failure.
    fn printed(&self, kept: Option<usize>) -> Option<Block> {
        let tail = match self.failure {
            Failure::Exited { exited } => &exited.tail,
            Failure::Red {
                red: RedCheck::Regression { tail, .. },
            } => tail,
            _ => return None,
        };
        (!tail.is_empty()).then(|| {
            Block::Evidence(Evidence {
                tail: tail.clone(),
                whole: None,
                kept,
            })
        })
    }

    /// What the node is, the attempt it stands at where `attempt` says, and how long it
    /// worked.
    fn facts(&self, attempt: bool) -> Fields {
        let node = self.node;
        let kind = match self.declared.map(|declared| &declared.kind) {
            Some(NodeKind::Check(builtin)) => format!("{}, {}", node.kind, builtin.builtin_name()),
            _ => node.kind.to_string(),
        };
        let attempt = node.attempt.filter(|_| attempt).map(|at| at.to_string());
        let took = time_taken(node).map(duration);
        Fields::new()
            .push_if(KIND, kind)
            .push_if(ATTEMPT, attempt.unwrap_or_default())
            .push_if(TOOK, took.unwrap_or_default())
    }
}
