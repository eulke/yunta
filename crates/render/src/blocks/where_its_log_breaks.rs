//! Where the log of a run that broke stops making sense, as its ended pane says it under its
//! heading: the event replay could not take, in replay's own words, on one row; then how far
//! the log reads, the last event replay applied, and the node the run waited on a person at
//! there, where it waited on one. Not what a run standing still waits on
//! ([`WhereItStopped`](super::WhereItStopped)): nothing moves a broken log again.

use surco::doc::Block;
use surco::line::Line;
use yunta_core::{NodeId, Seq};

/// What a broken run's pane says the event replay could not take under.
const WHERE_ITS_LOG_STOPS: &str = "where its log stops making sense";

/// Where a run's log stops making sense: replay's words of the event it could not take, the
/// last event it applied, and the node the run waited on a person at there.
pub struct WhereItsLogBreaks<'a> {
    pub diagnostic: &'a str,
    pub reads_up_to: Option<Seq>,
    pub waited_on: Option<&'a NodeId>,
}

impl WhereItsLogBreaks<'_> {
    /// Its blocks: the heading, then the event it could not take and how far it reads.
    pub fn blocks(&self) -> Vec<Block> {
        let broke = Line::new().plain(surco::text::one_line(self.diagnostic));
        let reads = self.reads_up_to.map(|seq| {
            let reads = format!("it reads up to seq {seq}");
            let reads = match self.waited_on {
                Some(node) => format!("{reads}, where it waited on `{node}`"),
                None => reads,
            };
            Line::new().plain(reads)
        });
        vec![
            Block::Heading(WHERE_ITS_LOG_STOPS.to_string()),
            Block::Rows(std::iter::once(broke).chain(reads).collect()),
        ]
    }
}

#[cfg(test)]
mod tests;
