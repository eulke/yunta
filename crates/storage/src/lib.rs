//! Embedded, append-only event log storage. SQLite is the only backend —
//! no dialect leaks past this crate's interface: every public method here
//! takes and returns `yunta_core` types, and a failure keeps the backend's
//! cause behind a type-erased error. Async code reaches the log through
//! [`AsyncStorage`], which runs every call on a blocking thread.

// A panic is a bug, never a fallible path: production returns a typed
// error instead of unwrapping, expecting, or panicking. Tests are the one
// place a failed assertion is meant to abort — `clippy.toml` lifts these
// there, and integration tests are separate crates this attribute never
// reaches.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

mod async_storage;
mod error;
mod store;

pub use async_storage::AsyncStorage;
pub use error::{Cause, Result, StorageError};
pub use store::{ChainVerification, Purge, RunListing, Storage};
