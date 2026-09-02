//! Embedded, append-only event log storage. SQLite is the only backend —
//! no dialect leaks past this crate's interface: every public method here
//! takes and returns `yunta_core` types, and a failure keeps the backend's
//! cause behind a type-erased error. Async code reaches the log through
//! [`AsyncStorage`], which runs every call on a blocking thread.

mod async_storage;
mod error;
mod store;

pub use async_storage::AsyncStorage;
pub use error::{Cause, Result, StorageError};
pub use store::{ChainVerification, Purge, RunListing, Storage};
