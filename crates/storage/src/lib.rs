#![forbid(unsafe_code)]

//! Embedded, append-only event log storage. SQLite is the only backend —
//! no dialect leaks past this crate's interface: every public method here
//! takes and returns `yunta_core` types, never rusqlite's.

mod error;
mod store;

pub use error::{Result, StorageError};
pub use store::{ChainVerification, Storage};
