//! A question open at the terminal of the process that drives a run, as its scene says it:
//! what is asked, under it the process that asks, by the id its registry names, and since
//! when, to the minute; then that nothing here answers it, and where it is answered. The
//! registry says it, not the log: a question asked live stands on no log until it is
//! answered there, which is why nothing a reader elsewhere does answers it.
//!
//! Its labels stand in the column a task's proof sets its own in; the words that say who
//! asks and where it is answered are those every saying of an asking shares
//! ([`crate::engine`]).

use surco::doc::Block;
use surco::line::{Line, Tone};
use yunta_replay::registry::Asking;

use super::task_proof::LABEL;

/// The label of the process that asks.
const PID: &str = "pid";

/// The label of the instant it began to ask.
const SINCE: &str = "since";

/// A question open at the terminal of the process that drives a run, as its scene says it.
/// Not replay's [`Asking`], what the run's registry says of it, which this says.
pub struct AskedAtItsTerminal<'a> {
    pub asking: &'a Asking,
}

impl AskedAtItsTerminal<'_> {
    /// What it says, top to bottom: what is asked, with the process that asks and since
    /// when on labelled rows under it; then that nothing here answers it, and where.
    pub fn blocks(&self) -> Vec<Block> {
        let since = self.asking.prompt.since.format(crate::TIME_OF_DAY_FORMAT);
        vec![
            Block::Heading(crate::engine::asking_on(self.asking)),
            Block::Rows(vec![
                labelled(PID, self.asking.pid.to_string()),
                labelled(SINCE, since.to_string()),
            ]),
            Block::Rows(vec![Line::new().plain(crate::engine::NOTHING_HERE_ANSWERS)]),
        ]
    }
}

/// `value` beside `label`, the label muted in its column.
fn labelled(label: &str, value: String) -> Line {
    Line::new()
        .push(Tone::Muted, format!("{label:<LABEL$}"))
        .plain(value)
}

#[cfg(test)]
mod tests;
