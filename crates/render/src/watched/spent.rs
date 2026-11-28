//! What a watched run has spent, as Surco's [`Meter`]: the figures it says, never their
//! drawing, which each surface makes with its own look.
//!
//! It is named by what it says, what the run spent, and not by the component that draws it.

use surco::components::{Meter, Time};
use surco::figures::{duration, Tokens};
use surco::words::{AT_WORK, WAITING_ON_YOU};
use yunta_replay::frame::{RunFrame, RunTimes};

/// What the run has spent: its tokens, one bar a node of what each spent in the workflow's
/// order, its time at work, and its time waiting on the person where it comes to a second,
/// as `times` measures them. It draws nothing: a terminal draws it through
/// [`Terminal::meter`](surco::surface::Terminal::meter), with its look.
pub fn spent(frame: &RunFrame, times: &RunTimes) -> Meter {
    let at_work = Time {
        figure: duration(times.time.working),
        label: AT_WORK.to_string(),
        by_person: false,
    };
    // A part under a second is no part of the run's time, as the run's page says it.
    let waited = (times.time.people.as_secs() > 0).then(|| Time {
        figure: duration(times.time.people),
        label: WAITING_ON_YOU.to_string(),
        by_person: true,
    });
    Meter {
        count: Tokens(frame.tokens.total()).to_string(),
        bars: frame
            .nodes
            .iter()
            .map(|node| node.tokens.total() as f64)
            .collect(),
        times: std::iter::once(at_work).chain(waited).collect(),
    }
}

#[cfg(test)]
mod tests;
