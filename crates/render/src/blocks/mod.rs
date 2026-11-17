//! The blocks a run's own facts are told in: what a failure says beyond
//! its node's row, what a node hands over, a task's proof and its scope,
//! the command each option of a decision names, a quoted refusal, and why
//! a task stopped. Each is said with Surco's components
//! ([`surco::components`]), so every surface draws it the way it draws
//! any other block.
//!
//! What a node hands over, a task's proof and its scope are what a node's
//! pane shows and its printed page prints, each read from the ledgers of
//! the run's state: [`HandsOver`] says each artifact the node declares
//! with where it stands on its open attempt, [`TaskProof`] a task's own
//! row and how it is proved, [`TaskScope`] the paths it may write and
//! which of them its sessions wrote. A task's proof and its scope stand
//! one step under the task's row; where the blocks around them stand is
//! their caller's.

pub mod decision;
pub mod diagnostic;
mod failure;
mod hands_over;
pub mod stop;
mod task_proof;
mod task_scope;

pub(crate) use decision::{departure_facts, departure_words};
pub use failure::FailureDetail;
pub(crate) use failure::{remedy_words, reroute_words, stuck_words};
pub use hands_over::HandsOver;
pub(crate) use task_proof::task_mark;
pub use task_proof::TaskProof;
pub use task_scope::TaskScope;

#[cfg(test)]
mod tests;
