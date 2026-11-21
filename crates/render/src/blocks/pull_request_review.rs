//! The pull request a gate waits on, as its scene says it once its forge answered a poll:
//! its number and its link, then how it stands, labelled in the column a node's own page
//! sets its labels in.
//!
//! How it stands is what the forge answered, read as the engine reads a poll on its next
//! wake: a review decides only the commit it reviewed, so an approval or a request for
//! changes left on an earlier commit leaves the pull request waiting on a review of its
//! last; a merge, or a close without one, stands whatever was reviewed. Where no forge
//! answered, it says why: the run declares none, the variable its token is in is not set
//! here, or the forge failed, in its own words.

use surco::doc::Block;
use surco::line::{Line, Tone};
use surco::width::LABEL_WIDTH;
use yunta_core::port::{ForgeUnanswered, PolledGate, PullRequestRef, ReviewOutcome};

use super::how_it_stands::STANDS;

/// The words a pull request is named by, before its number and its link.
const PULL_REQUEST: &str = "pull request";

/// How a pull request stands while no review decides its last commit.
const WAITING: &str = "waiting on its review";

/// How a pull request stands whose last review was left on an earlier commit than its last.
const WAITING_ON_ITS_LAST: &str = "waiting on a review of its last commit";

/// How a pull request a person closed without merging stands.
const CLOSED: &str = "closed without merging";

/// What a pull request's standing opens with where no forge answered.
const NOT_KNOWN: &str = "not known";

/// The pull request a gate waits on, and how it stands as its forge answered, or why no
/// forge answered. Not the forge's [`ReviewOutcome`], which this says, nor the outcome a
/// `pull_request` node closes with, which names the pull request it opened.
pub struct PullRequestReview<'a> {
    pub pull_request: &'a PullRequestRef,
    pub answered: &'a Result<PolledGate, ForgeUnanswered>,
}

impl PullRequestReview<'_> {
    /// Its rows, one block: the pull request by its number and its link, then how it stands.
    pub fn blocks(&self) -> Vec<Block> {
        let named = crate::outcome::pull_request(self.pull_request.number, &self.pull_request.url);
        let link = Line::new().plain(format!("{PULL_REQUEST} {named}"));
        let stands = Line::new()
            .push(Tone::Muted, format!("{STANDS:<LABEL_WIDTH$}"))
            .plain(" ")
            .plain(stands(self.answered));
        vec![Block::Lines(vec![link, stands])]
    }
}

/// How a pull request stands, as `answered` says it.
fn stands(answered: &Result<PolledGate, ForgeUnanswered>) -> String {
    match answered {
        Ok(polled) => reviewed(polled),
        Err(ForgeUnanswered::Undeclared) => format!("{NOT_KNOWN}: the run declares no forge"),
        Err(ForgeUnanswered::TokenUnset { token_env }) => format!(
            "{NOT_KNOWN}: `{token_env}`, the variable its forge's token is in, is not set here"
        ),
        Err(ForgeUnanswered::Failed { said }) => format!("{NOT_KNOWN}: {said}"),
    }
}

/// How a pull request stands as its forge answered a poll: its review where one decides its
/// last commit, its merge, its close, or that it waits on a review.
fn reviewed(polled: &PolledGate) -> String {
    match &polled.review {
        ReviewOutcome::Pending => WAITING.to_string(),
        ReviewOutcome::Approved { by, reviewed_sha } if *reviewed_sha == polled.head_sha => {
            format!("approved by {by}")
        }
        ReviewOutcome::ChangesRequested {
            by, reviewed_sha, ..
        } if *reviewed_sha == polled.head_sha => format!("changes asked by {by}"),
        ReviewOutcome::Approved { .. } | ReviewOutcome::ChangesRequested { .. } => {
            WAITING_ON_ITS_LAST.to_string()
        }
        ReviewOutcome::Closed => CLOSED.to_string(),
        ReviewOutcome::Merged { by, .. } => format!("merged by {by}"),
    }
}

#[cfg(test)]
mod tests;
