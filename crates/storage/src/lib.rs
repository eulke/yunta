#![forbid(unsafe_code)]

//! Embedded, append-only event log storage. SQLite is the only backend —
//! no dialect leaks past this crate's interface: every public method here
//! takes and returns `yunta_core` types, never rusqlite's.

mod error;
mod store;

pub use error::{Result, StorageError};
pub use store::{ChainVerification, Storage};

/// Identifies this crate to integration tests elsewhere in the workspace.
pub const CRATE_NAME: &str = "yunta-storage";

/// Name of the crate this one depends on, used by an integration test
/// to prove the `storage → core` edge is wired and not just declared.
pub fn depends_on() -> &'static str {
    yunta_core::CRATE_NAME
}
