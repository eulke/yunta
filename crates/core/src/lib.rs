#![forbid(unsafe_code)]

//! Shared domain types for the Yunta workspace.
//!
//! `yunta-core` is the bottom of the dependency graph (T0.1): every other
//! crate may depend on it, and it depends on nothing else in the workspace.
//! Its real content — newtyped identifiers, error types, `Clock` — lands in
//! later tasks (T0.3 and beyond); for now it only carries an identity marker
//! so the workspace wiring itself is testable.

/// Identifies this crate to integration tests elsewhere in the workspace.
pub const CRATE_NAME: &str = "yunta-core";
