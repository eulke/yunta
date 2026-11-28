//! The run a promotion handed a run's work to, as the promoted run's ended pane says it under
//! its heading: the successor's row, its mark and its handle, then its workflow and its mode,
//! its word and how long it has stood there, as the headline across its own screen says them;
//! or, where none came in the time one takes to be created, that none came.

use surco::doc::Block;
use surco::glyphs::Glyphs;
use surco::line::{Line, Tone};
use yunta_replay::frame::{RunFrame, RunTimes};

use crate::watched::stood;

/// What the run a promotion handed the work to stands under.
const ITS_SUCCESSOR: &str = "its successor";

/// What a promoted run's pane says where no successor came in the time one takes to be
/// created, as its follower waits for one.
const NONE_CAME: &str = "none came in the time one takes to be created";

/// The run a promotion handed a run's work to. Not the promotion, which says why the run
/// handed it on.
pub enum ItsSuccessor<'f> {
    /// It came: the successor, framed, and what the clock adds to its frame.
    Came(&'f RunFrame, &'f RunTimes),
    /// None came in the time one takes to be created.
    NoneCame,
}

impl ItsSuccessor<'_> {
    /// Its blocks, in `glyphs`: the heading, then the successor's row or that none came.
    pub fn blocks(&self, glyphs: Glyphs) -> Vec<Block> {
        let row = match self {
            ItsSuccessor::Came(frame, times) => row(frame, times, glyphs),
            ItsSuccessor::NoneCame => Line::new().plain(NONE_CAME),
        };
        vec![
            Block::Heading(ITS_SUCCESSOR.to_string()),
            Block::Rows(vec![row]),
        ]
    }
}

/// The successor `frame` as a row: its mark and its handle, then what its headline says,
/// how long it has stood as `times` measures it.
fn row(frame: &RunFrame, times: &RunTimes, glyphs: Glyphs) -> Line {
    let word = frame.standing;
    let mark = word.mark();
    let sep = glyphs.sep();
    let called = crate::state::called(&frame.workflow, &frame.mode);
    let said = Line::new()
        .push(Tone::of(mark), glyphs.mark(mark).to_string())
        .plain(" ")
        .push(Tone::Strong, frame.run_id.handle())
        .plain(format!("  {called} {sep} {}", word.word()));
    match stood(frame, times) {
        Some(stood) => said.plain(format!(" {sep} {stood}")),
        None => said,
    }
}

#[cfg(test)]
mod tests;
