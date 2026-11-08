//! What Yunta says in Surco: the sentences a log's typed facts are read as (why
//! a run paused, how a node ended, what a failure says, which fallback the engine
//! took), and the documents a person reads a run by: a gate's page, a plan, a
//! pull request's body, the receipt.
//!
//! One crate, because two readers told the same fact in two words — a person at
//! a terminal, a file a run leaves, a pull request — read it side by side. It
//! sits on `yunta-core`, `yunta-replay` and `yunta-surco`: the engine hands core
//! data, replay reads what a run is from its log, and the CLI and the adapters
//! draw both with these words and blocks. Nothing here reads a disk, a clock or
//! a process, which `tests/pure.rs` holds: what was read, and what depends on the
//! process — which stream is a terminal, how wide it measures, what the
//! environment asked for — is the caller's to settle and pass in.
//!
//! How anything is drawn is not decided here: the words of a state, the
//! marks, the figures, the blocks a document is made of and the surfaces
//! that draw them are Surco's ([`surco`]). This crate says Yunta's own
//! facts with them, and never writes a second version of one.
//!
//! Its map:
//!
//! - A run and its nodes: `state`, `pause`, `outcome`, `failure`, `spend`,
//!   `changes`, `caution`, `inputs`, `place`, `target` and `policy`.
//! - A decision a person meets: `gate`, `escalation`, `claim`, `evidence`,
//!   `offers`, `shown`, `plan`, `spec`, `findings` and `found`.
//! - What leaves the terminal: `blocks`, `published` and `receipt`.
//! - What a reader is told to type: `command`, every command written once.

pub mod blocks;
pub mod caution;
pub mod changes;
pub mod claim;
pub mod command;
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
pub mod plan;
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
    mod desk;
    mod documents;
    mod failure;
    mod failure_families;
    mod findings;
    mod found;
    mod grant;
    mod inputs;
    mod offers;
    mod publication;
    mod publication_text;
    mod retry;
    mod routes;
}
