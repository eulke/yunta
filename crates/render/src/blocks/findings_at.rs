//! The run's findings as a run's pane shows them, at the one pointed at: how many were
//! answered of how many were posted and which nodes posted them, which the pane is titled by;
//! a finding a row, the pointer on the one pointed at, an answered one keeping its row with
//! its answer under it; then the finding pointed at: the node that posted it and where, in
//! a column, what it finds, wrapped, and its answer once it has one, after a blank row and
//! led by the done mark. Which finding is pointed at is the caller's to say.
//!
//! A pane short of rows takes the parts apart ([`FindingsAt::list`],
//! [`FindingsAt::pointed`]): the list stands without the finding pointed at, and the title's
//! count stands for the list where it gives way.

use surco::doc::Block;
use surco::glyphs::{Glyphs, Mark};
use surco::line::{Line, Tone};
use yunta_core::events::findings::{RunFindings, StandingFinding};
use yunta_core::{FindingId, NodeId};

use crate::found;
use crate::page::findings::{heading, list};

/// The cells the node that posted the finding pointed at stands in, before where it stands:
/// the longest of fragua's ids that posts one, `review@reviewer-alt`, and two that part it
/// from its place.
const POSTED_BY: usize = 21;

/// What the place of a finding the engine posted about the run, and not a node, is posted by.
const THE_RUN: &str = "the run";

/// The run's findings as its pane shows them, at the one pointed at. Not the findings' page,
/// [`crate::page::Findings`], which says each finding whole.
pub struct FindingsAt<'v> {
    view: &'v RunFindings,
    /// Where the finding pointed at stands among them, where one is.
    at: Option<usize>,
}

impl<'v> FindingsAt<'v> {
    /// The run's findings `view`, at the one `pointed` names by the node that posted it, none
    /// for one the engine posted, and its id among that node's: none pointed at where it
    /// names none of them.
    pub fn of(view: &'v RunFindings, pointed: Option<(Option<&NodeId>, &FindingId)>) -> Self {
        let at = pointed.and_then(|(node, id)| {
            view.findings
                .iter()
                .position(|standing| standing.node.as_ref() == node && standing.finding.id == *id)
        });
        FindingsAt { view, at }
    }

    /// Its heading, which the pane is titled by, one part cut at the row's edge rather than
    /// parted: how many were answered of how many were posted, then the nodes that posted
    /// them.
    pub fn heading(&self, glyphs: Glyphs) -> String {
        heading(self.view, glyphs)
    }

    /// Its list, a finding a row, the pointer on the one pointed at.
    pub fn list(&self) -> Block {
        let listed = list(self.view);
        let listed = match self.at {
            Some(at) => listed.pointing_at(at),
            None => listed,
        };
        Block::Findings(listed)
    }

    /// The finding pointed at: the node that posted it in its column and where it stands,
    /// then what it finds, the words going on in the rows under them; its answer, once it has
    /// one, apart and led by the done mark. Nothing where none is pointed at.
    pub fn pointed(&self, glyphs: Glyphs) -> Vec<Block> {
        let Some(standing) = self.at.and_then(|at| self.view.findings.get(at)) else {
            return Vec::new();
        };
        let finding = &standing.finding;
        let by = standing.node.as_ref().map_or(THE_RUN, NodeId::as_str);
        let place = Line::new()
            .plain(format!("{by:<POSTED_BY$}"))
            .push(Tone::Command, finding.location.to_string());
        let (_, detail) = found::said(&finding.says);
        let found = Block::Lines(vec![place, Line::new().plain(detail)]);
        match answered(standing, glyphs) {
            Some(answer) => vec![found, answer],
            None => vec![found],
        }
    }

    /// Every part under its heading: its list, and the finding pointed at.
    pub fn blocks(&self, glyphs: Glyphs) -> Vec<Block> {
        std::iter::once(self.list())
            .chain(self.pointed(glyphs))
            .collect()
    }
}

/// The latest answer `standing` was given, led by the done mark, as its row in the list says
/// it: none while no node answered it.
fn answered(standing: &StandingFinding, glyphs: Glyphs) -> Option<Block> {
    let given = standing.answers.last()?;
    let answer = Line::new()
        .push(Tone::of(Mark::Done), glyphs.mark(Mark::Done).to_string())
        .plain(format!(" {}: {}", given.answer.as_str(), given.why));
    Some(Block::Lines(vec![answer]))
}

#[cfg(test)]
mod tests;
