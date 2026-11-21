//! The findings that keep a run that finished from being done, as its receipt's pane says
//! them under a report: under their heading, a finding a row as the findings' list draws
//! one, then the first of them as the findings' pane draws the one pointed at, the node that
//! posted it and where, then what it finds. Which findings block is the run's state's to say
//! (`RunState::findings_that_block`), as the run's frame and its receipt count them, so the
//! rows and the count beside the run's word never disagree.

use surco::doc::Block;
use surco::glyphs::Glyphs;
use yunta_core::events::findings::RunFindings;
use yunta_replay::RunState;

use super::FindingsAt;
use crate::page::findings::list;

/// What the findings that block a run stand under.
const THE_FINDINGS_THAT_BLOCK: &str = "the findings that block";

/// The findings that keep a run that finished from being done, as they stand. Not
/// [`FindingsAt`], every finding of the run at the one pointed at.
pub struct FindingsThatBlock {
    view: RunFindings,
}

impl FindingsThatBlock {
    /// The findings that block the run `state` folds, each as it stands, in the order each
    /// was first posted.
    pub fn of(state: &RunState) -> FindingsThatBlock {
        let blocking = state.findings_that_block();
        let findings = state.findings.standing().findings.into_iter();
        let blocks = |standing: &yunta_core::events::findings::StandingFinding| {
            blocking.iter().any(|posted| {
                posted.node == standing.node && posted.finding.id == standing.finding.id
            })
        };
        FindingsThatBlock {
            view: RunFindings {
                findings: findings.filter(blocks).collect(),
            },
        }
    }

    /// Its blocks, in `glyphs`: none where no finding blocks the run; else their heading and a
    /// finding a row, then the first of them, where it stands and what it finds.
    pub fn blocks(&self, glyphs: Glyphs) -> Vec<Block> {
        let Some(first) = self.view.findings.first() else {
            return Vec::new();
        };
        let first = FindingsAt::of(&self.view, Some((first.node.as_ref(), &first.finding.id)));
        let listed = Block::Findings(list(&self.view));
        [Block::Heading(THE_FINDINGS_THAT_BLOCK.to_string()), listed]
            .into_iter()
            .chain(first.pointed(glyphs))
            .collect()
    }
}
