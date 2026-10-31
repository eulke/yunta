//! How Yunta tells a reader things: the columns a line is built from,
//! the step every block is indented by, the bars and sparklines that
//! carry a magnitude, the words a run's and a node's state and a
//! decision's options are called by, the sentences a log's typed facts
//! are read as (why a run paused, how a node ended, what a failure says,
//! which fallback the engine took), and the characters it is all drawn
//! with.
//!
//! One crate, because two readers told the same fact in two words — a
//! person at a terminal, a file the engine writes, a pull request — read
//! it side by side. It sits on `yunta-core` alone, below the engine, so
//! the engine writes for a person with the same words and blocks the
//! command line draws. What depends on the process — which stream is a
//! terminal, how wide it measures, what the environment asked for — is
//! the caller's to settle and pass in.
//!
//! How anything is drawn is not decided here: the words of a state, the
//! marks, the figures, the blocks a document is made of and the surfaces
//! that draw them are Surco's ([`surco`]). This crate says Yunta's own
//! facts with them, and never writes a second version of one.

pub mod blocks;
pub mod caution;
pub mod claim;
pub mod escalation;
pub mod evidence;
pub mod failure;
mod findings;
pub mod found;
pub mod gate;
pub mod inputs;
pub mod offers;
pub mod outcome;
pub mod pause;
pub mod place;
mod plan;
pub mod policy;
pub mod published;
pub mod receipt;
pub mod shown;
mod spec;
pub mod spend;
pub mod state;
pub mod target;

#[cfg(test)]
mod tests {
    mod cards;
    mod cautions;
    mod claims;
    mod console_page;
    mod deciding;
    mod documents;
    mod failure;
    mod failure_families;
    mod findings;
    mod found;
    mod grant;
    mod inputs;
    mod offers;
    mod publication;
    mod retry;
}
