//! Why one declared artifact did not close.
//!
//! A file that was never written and a file whose content is wrong are
//! different failures, and the difference is the whole reason the repair
//! cycle can exist: writing the document again fixes the second and can
//! never reach the first. Giving them one type made that distinction a
//! predicate somebody had to remember to call; giving them two variants
//! makes it a fact the compiler carries.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::Report;
use crate::NodeId;

/// Why one declared artifact did not close.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "failure", rename_all = "kebab-case")]
pub enum ArtifactFailure {
    /// The file itself. No rewrite of its content reaches this.
    File { path: String, problem: FileProblem },
    /// The file is there and its content is not what its kind declares.
    Content(Report),
}

impl ArtifactFailure {
    pub fn file(path: impl Into<String>, problem: FileProblem) -> Self {
        ArtifactFailure::File {
            path: path.into(),
            problem,
        }
    }

    /// Where a reader opens the file.
    pub fn path(&self) -> &str {
        match self {
            ArtifactFailure::File { path, .. } => path,
            ArtifactFailure::Content(report) => &report.document.path,
        }
    }

    /// The report, when the failure is about content — the form a
    /// receipt counts and a repair instruction is built from.
    pub fn report(&self) -> Option<&Report> {
        match self {
            ArtifactFailure::File { .. } => None,
            ArtifactFailure::Content(report) => Some(report),
        }
    }

    /// Whether writing the document again could fix this.
    pub fn is_repairable(&self) -> bool {
        self.report().is_some_and(|report| !report.is_empty())
    }
}

impl fmt::Display for ArtifactFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArtifactFailure::File { path, problem } => f.write_str(&crate::text::problems(
                path,
                &[format!("the document {problem}")],
            )),
            ArtifactFailure::Content(report) => report.fmt(f),
        }
    }
}

/// What is wrong with the file, before anything inside it is read.
///
/// Each renders as the rest of the sentence "the document ...", because
/// there is no entry inside to blame and pointing at one would be an
/// invention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "file-problem", rename_all = "kebab-case")]
pub enum FileProblem {
    /// Declared by a node and never produced.
    Missing { node: NodeId },
    /// Produced with no content.
    Empty,
    /// Past `limits.max_artifact_bytes`. Both numbers are on the table,
    /// because a runaway artifact is a fact about the run and a
    /// truncation would hide it.
    Oversized { bytes: u64, ceiling: u64 },
    /// Exists, and the filesystem refused it.
    Unreadable { detail: String },
}

impl FileProblem {
    /// The stable name of this kind of problem: what a receipt counts
    /// and a log is grepped by, unaffected by any rewording.
    pub fn code(&self) -> &'static str {
        match self {
            FileProblem::Missing { .. } => "artifact-missing",
            FileProblem::Empty => "artifact-empty",
            FileProblem::Oversized { .. } => "artifact-oversized",
            FileProblem::Unreadable { .. } => "artifact-unreadable",
        }
    }
}

impl fmt::Display for FileProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileProblem::Missing { node } => {
                write!(f, "was declared by node `{node}` and never produced")
            }
            FileProblem::Empty => f.write_str("is empty; a declared artifact must have content"),
            FileProblem::Oversized { bytes, ceiling } => write!(
                f,
                "is {bytes} bytes; `limits.max_artifact_bytes` is {ceiling}"
            ),
            FileProblem::Unreadable { detail } => write!(f, "exists but cannot be read: {detail}"),
        }
    }
}
