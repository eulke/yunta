//! Why a node failed, as data.
//!
//! The one payload field that is not a plain value: a node fails either
//! with a sentence the engine states, or with declared artifacts that
//! did not close, each carrying its own problems. Keeping both in one
//! type is what lets the log record the facts and every surface produce
//! its own prose from them, instead of the engine writing prose once and
//! three surfaces repairing it.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::diagnostic::{ArtifactFailure, Report};

/// Why a node failed.
///
/// Untagged, with `Message` last: a payload carrying `artifacts:` reads
/// as [`Failure::Artifacts`], and a log written before failures were
/// data carries `outcome:` alone and reads back as
/// [`Failure::Message`]. That tolerance is the rule for what is
/// persisted and versioned, and it is why no reader needs to know which
/// version wrote the line it is looking at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum Failure {
    /// Declared artifacts that did not close, each with its own
    /// problems.
    Artifacts { artifacts: Vec<ArtifactFailure> },
    /// A failure the engine states in one sentence.
    Message { outcome: String },
}

impl Failure {
    pub fn message(text: impl Into<String>) -> Self {
        Failure::Message {
            outcome: text.into(),
        }
    }

    pub fn artifacts(failures: Vec<ArtifactFailure>) -> Self {
        Failure::Artifacts {
            artifacts: failures,
        }
    }

    /// Every report behind this failure, each carrying the document it
    /// is about. What a receipt counts and a repair instruction is
    /// built from.
    pub fn reports(&self) -> impl Iterator<Item = &Report> {
        self.failures().filter_map(ArtifactFailure::report)
    }

    /// Every declared artifact that did not close. Empty for a failure
    /// that has nothing to do with artifacts.
    pub fn failures(&self) -> impl Iterator<Item = &ArtifactFailure> {
        match self {
            Failure::Artifacts { artifacts } => artifacts.iter(),
            Failure::Message { .. } => [].iter(),
        }
    }
}

impl fmt::Display for Failure {
    /// The prose a reader sees, produced here rather than stored: one
    /// sentence for a plain failure, and one block per failing document
    /// in declaration order for an artifact failure.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Message { outcome } => f.write_str(outcome),
            Failure::Artifacts { artifacts } => {
                for (position, artifact) in artifacts.iter().enumerate() {
                    if position > 0 {
                        f.write_str("\n")?;
                    }
                    write!(f, "{artifact}")?;
                }
                Ok(())
            }
        }
    }
}
