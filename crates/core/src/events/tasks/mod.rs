//! The tasks document, as a run works it: a task registered, and a task
//! that reached a new status.

pub mod kinds;
pub mod payloads;

pub use kinds::TaskEvent;
pub use payloads::*;
