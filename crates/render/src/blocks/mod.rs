//! The blocks a run's own facts are told in: what a failure says beyond
//! its node's row, the command each option of a decision names, a quoted
//! refusal, and why a task stopped. Each is said with Surco's components
//! ([`surco::components`]), so every surface draws it the way it draws
//! any other block.

pub mod decision;
pub mod diagnostic;
mod failure;
pub mod stop;

pub(crate) use decision::{departure_facts, departure_words};
pub use failure::FailureDetail;
pub(crate) use failure::{remedy_words, reroute_words, stuck_words};

#[cfg(test)]
mod tests;
