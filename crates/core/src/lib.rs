#![forbid(unsafe_code)]

//! Shared domain types for the Yunta workspace.
//!
//! `yunta-core` is the bottom of the dependency graph (T0.1): every other
//! crate may depend on it, and it depends on nothing else in the workspace.
//! Newtyped identifiers and the `Clock` trait land in later tasks; error
//! types (T0.3) are here from the start since every other crate needs them.

mod error;

pub use error::{Result, YuntaError};

/// Identifies this crate to integration tests elsewhere in the workspace.
pub const CRATE_NAME: &str = "yunta-core";
