//! What Yunta refuses, said: the sentences a person and an agent read for a
//! refusal, over the data core holds of it.
//!
//! A refusal is what is true of a run, a request or a workflow, and a reader
//! does something else; the engine and the commands return it as data, and
//! every interface says it here, so a sentence is written once. [`Refused`]
//! is one refusal as a value: what a person and an agent read of it, what
//! goes on from it, and its page. [`check`] says
//! what `yunta check` refuses about a workflow; the rest says a decision or a
//! close a run refuses, a run another engine holds, a command's inputs and a
//! pack's install.
//!
//! Not a failure, which `failure` says: something below that broke.

pub mod check;
mod refused;
mod words;
mod wrapped;

#[cfg(test)]
mod tests;

pub use refused::{Candidate, Refused};
pub use words::{
    already_installed, answers, close, gate, hold_unclear, inputs, moving, not_installed,
    refused_answer, stale_lock_taken, unnamed_committer, unrunnable,
};
