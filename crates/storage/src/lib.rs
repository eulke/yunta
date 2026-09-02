//! Embedded, append-only event log storage. SQLite is the only backend —
//! no dialect leaks past this crate's interface: every public method here
//! takes and returns `yunta_core` types, and a failure keeps the backend's
//! cause behind a type-erased error.

mod error;
mod store;

pub use error::{Cause, Result, StorageError};
pub use store::{ChainVerification, Purge, RunListing, Storage};
