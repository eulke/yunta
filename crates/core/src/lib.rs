#![forbid(unsafe_code)]

//! Shared domain types for the Yunta workspace.
//!
//! `yunta-core` is the bottom of the dependency graph (T0.1): every other
//! crate may depend on it, and it depends on nothing else in the workspace.
//! Newtyped identifiers and the `Clock` trait land in later tasks; error
//! types (T0.3) are here from the start since every other crate needs them.

mod capabilities;
mod clock;
mod config;
mod error;
pub mod events;
mod ids;
mod ledger;
mod manifest;
mod workflow;

pub use capabilities::Capabilities;
pub use clock::{Clock, SystemClock};
pub use config::{
    AdapterSettings, ConfigLayer, DefaultsConfig, Isolation, PathsConfig, RunnerCandidate,
    StorageConfig,
};
pub use error::{Result, YuntaError};
pub use ids::{NodeId, RunId, SessionId, TaskId};
pub use ledger::{Ledger, Task};
pub use manifest::{content_hash, sha256_hex, Manifest};
pub use workflow::{
    ArtifactKind, ArtifactSpec, Artifacts, HookFailurePolicy, HookStep, Hooks, Node, NodeDefaults,
    NodeKind, OnFailure, PromptSource, Workflow,
};

/// Identifies this crate to integration tests elsewhere in the workspace.
pub const CRATE_NAME: &str = "yunta-core";
