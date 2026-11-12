//! What Yunta says in Surco: the sentences a log's typed facts are read as (why
//! a run paused, how a node ended, what a failure says, which fallback the engine
//! took), and every page a person reads: a run's, a gate's, a plan, a pull
//! request's body, the receipt, and what each command answers.
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
//! - A run and its nodes: `state`, `pause` (what the log parked a run on),
//!   `engine` (what holds a run its log says moves: an engine gone, or one
//!   asking at its terminal), `outcome`, `failure`, `spend`, `changes`,
//!   `caution`, `inputs`, `place`, `target`, `policy`, and `progress`, how far
//!   it has come and where it stands on one line, and `children`, the runs
//!   it bore, under the nodes that bore them.
//! - What happened to a run, in order: `chronicle`, what each moment of it
//!   says, the same on every surface that follows the run; and `watched`,
//!   the rows a person watches a moving run by: what needs them, each node
//!   at work with what its session said, and the counters.
//! - A decision a person meets: `escalation`, `claim`, `evidence`, `offers`
//!   and `found`.
//! - What Yunta refuses: `refusal`, the sentences of every refusal over its data.
//! - How a run starts and ends: `start`, the line a run starts with, created,
//!   resumed or detached; and `ending`, what a cancel or a close says while it
//!   stops a run's engine and gives its checkouts back, and what stops it.
//! - What leaves the terminal: `blocks` and `published`.
//! - What a prompt says: `prompt`, what Escape does where it opens, the keys a
//!   list answers to, and what a command says with no terminal to ask on.
//! - What the application's screens say around their pages: `screen`, the trail
//!   every screen starts at and each stage's keys.
//! - What a reader is told to type: `command`, every command written once,
//!   and `declare`, what to declare in a project's config.
//! - What a machine reads: `document`, every versioned JSON document of a
//!   run, its stats, a decision and a refusal, the same from every command
//!   and tool.
//! - A workflow drawn: `graph`, its nodes and edges as Mermaid and as DOT,
//!   labelled with what a run's log derives when the graph is a run's.
//! - Every page a command shows, in the one form it takes: `page`, with a
//!   run's and its nodes', a workflow's graph, what each command answers, a
//!   pack's, the receipt, the findings, a gate's page, and what a decision
//!   shows: a plan, read at its desk, a spec, or any document.

pub mod blocks;
pub mod caution;
pub mod changes;
pub mod children;
pub mod chronicle;
pub mod claim;
pub mod command;
pub mod declare;
pub mod document;
pub mod ending;
pub mod engine;
pub mod escalation;
pub mod evidence;
pub mod failure;
pub mod found;
pub mod graph;
pub mod inputs;
pub mod offers;
pub mod outcome;
pub mod page;
pub mod pause;
pub mod place;
pub mod policy;
pub mod progress;
pub mod prompt;
pub mod published;
pub mod refusal;
pub mod screen;
pub mod spend;
pub mod start;
pub mod state;
pub mod target;
pub mod watched;

/// How an instant reads to a person: the day and the minute, in UTC, as a
/// log's clock keeps it — a limit's reset, when a page was drawn.
pub(crate) const INSTANT_FORMAT: &str = "%Y-%m-%d %H:%M UTC";

#[cfg(test)]
mod tests {
    mod cards;
    mod catalog;
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
    mod initialized;
    mod inputs;
    mod offers;
    mod packs;
    mod publication;
    mod publication_text;
    mod retry;
    mod routes;
    mod screen;
    mod setup;
}
