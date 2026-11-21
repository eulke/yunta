//! What a card on a spent cap rests on: the cap, by the limit the run declares it under, then
//! what was spent against it, each value in one column; a loop's says too that its tasks
//! document still holds ready tasks, which is why it wants another iteration.
//!
//! The cap is the escalation's: the run's token cap and a loop's iteration cap asked at the
//! engine's terminal carry the cap and what was spent in their menus. A loop that failed at
//! its cap with no terminal to ask carries the cap alone in its failure: the engine fails it
//! at the first pass with ready tasks past the cap, so what it ran is read as the cap, which
//! it is unless a pass past the cap only answered departures. Where a menu asks anything
//! else, there is none.
//!
//! The words before the values stand in one column, a fact that holds no value among them,
//! and the values two cells past the widest, as Surco lays labelled rows out: where the total
//! of a cap cannot stand in its row, it gives way and the cap stands.

use surco::components::{Labelled, LabelledRow};
use surco::doc::Block;
use surco::line::{Line, Tone};
use surco::width::INDENT;
use yunta_core::events::{Failure, GateWaitingPayload, Menu};

/// The label of the run's token cap.
pub(crate) const TOKEN_CAP: &str = "limits.max_tokens_per_run";

/// The label of the tokens a run spent, as the log adds them up.
pub(crate) const TOKENS_SPENT: &str = "total input+output tokens derived from the event log";

/// The label of a loop's iteration cap.
pub(crate) const ITERATION_CAP: &str = "limits.max_loop_iterations";

/// The label of the iterations a loop ran since it was last started.
pub(crate) const ITERATIONS_RUN: &str = "iterations already run this invocation";

/// Why a loop at its cap wants another iteration.
pub(crate) const TASKS_READY: &str = "the tasks document still has ready tasks";

/// What was spent against a cap, as a card on it rests on it. Not [`Menu::Budget`] or
/// [`Menu::LoopCap`], the engine's menus that ask about one, nor what a run's numbers say it
/// spent.
pub struct SpentAgainstItsCap {
    /// The cap, then what was spent, each by its label.
    spent: [(&'static str, String); 2],
    /// Why the run wants to go past it, where its menu says.
    wanting: Option<&'static str>,
}

impl SpentAgainstItsCap {
    /// The cap `escalation` asks about and what was spent against it: the run's token cap,
    /// or a loop's iteration cap asked at the engine's terminal or failed at with none. None
    /// for any other escalation.
    pub fn asked_by(escalation: &GateWaitingPayload) -> Option<SpentAgainstItsCap> {
        match escalation.menu()? {
            Menu::Budget(cap) => Some(SpentAgainstItsCap {
                spent: [
                    (TOKEN_CAP, cap.cap.to_string()),
                    (TOKENS_SPENT, cap.spent.to_string()),
                ],
                wanting: None,
            }),
            Menu::LoopCap(cap) => Some(iterations(cap.cap, cap.ran)),
            Menu::Failure(menu) => match &menu.failure {
                Failure::LoopCapped { loop_cap, .. } => Some(iterations(*loop_cap, *loop_cap)),
                _ => None,
            },
            Menu::Gate(_)
            | Menu::Console(_)
            | Menu::Published(_)
            | Menu::Scope(_)
            | Menu::Departure(_) => None,
        }
    }

    /// Its blocks: a row each, one step in, the cap and what was spent with their values in
    /// one column, then why the run wants to go past it, where it says; a row whose value
    /// cannot stand in its row at the width it is drawn at gives way.
    pub fn blocks(&self) -> Vec<Block> {
        let label = |words: &str, tone: Tone| Line::new().plain(INDENT).push(tone, words);
        let spent = self.spent.iter().map(|(words, value)| LabelledRow {
            label: label(words, Tone::Muted),
            value: Some(value.clone()),
        });
        let wanting = self.wanting.map(|wanting| LabelledRow {
            label: label(wanting, Tone::Plain),
            value: None,
        });
        let rows = spent.chain(wanting).collect();
        vec![Block::Labelled(Labelled { rows })]
    }
}

/// A loop's iteration cap `cap`, with the iterations `ran` against it, and its ready tasks.
fn iterations(cap: u32, ran: u32) -> SpentAgainstItsCap {
    SpentAgainstItsCap {
        spent: [
            (ITERATION_CAP, cap.to_string()),
            (ITERATIONS_RUN, ran.to_string()),
        ],
        wanting: Some(TASKS_READY),
    }
}

#[cfg(test)]
mod tests;
