//! Why one declared artifact did not close.
//!
//! A file that was never written, a file whose content is wrong and an
//! artifact another run owes are three failures, and every surface reads
//! them differently: one says the node produced nothing, one says what
//! the document it produced got wrong, and one says nothing this run
//! does reaches the artifact at all. Three variants make that a fact the
//! compiler carries rather than a predicate somebody has to remember to
//! call.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::Report;
use crate::events::ArtifactId;
use crate::{NodeId, RunId};

/// Why one declared artifact did not close.
///
/// The variants answer the one question that decides what is worth
/// doing next: rewriting the content reaches [`Content`](Self::Content),
/// nothing this node writes reaches [`File`](Self::File), and nothing
/// this run does at all reaches [`Unheld`](Self::Unheld), because the
/// artifact is another run's to produce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "failure", rename_all = "kebab-case")]
pub enum ArtifactFailure {
    /// The file itself. No rewrite of its content reaches this.
    File { path: String, problem: FileProblem },
    /// The file is there and its content is not what its kind declares.
    Content(Report),
    /// Declared here and held by no run that could hand it over: the
    /// run that owes it — a `kind: workflow` node's child, or the run a
    /// mount names as its source — answers with nothing under this
    /// identity.
    ///
    /// A node of composition writes no file, so there is nothing on
    /// disk to be missing and nothing this run does reaches the
    /// artifact: producing it is another run's to do.
    Unheld {
        /// The run that was asked and holds none of it — where a reader
        /// goes to look.
        run: RunId,
        /// The node of that run the artifact was asked of, when the
        /// reference names one. `None` when the question is about the
        /// run as a whole, which is what a `kind: workflow` node asks
        /// of its child.
        producer: Option<NodeId>,
        /// What the run was asked for: the identity, which is what a
        /// log answers by.
        artifact: ArtifactId,
    },
}

impl ArtifactFailure {
    pub fn file(path: impl Into<String>, problem: FileProblem) -> Self {
        ArtifactFailure::File {
            path: path.into(),
            problem,
        }
    }

    /// The stable name of this failure: what a receipt counts and a log
    /// is grepped by, unaffected by any rewording.
    ///
    /// `None` for a content failure, which is not one problem but every
    /// problem the document has, each carrying its own code.
    pub fn code(&self) -> Option<&'static str> {
        match self {
            ArtifactFailure::File { problem, .. } => Some(problem.code()),
            ArtifactFailure::Unheld { .. } => Some("artifact-unheld"),
            ArtifactFailure::Content(_) => None,
        }
    }

    /// Where a reader opens the file. `None` when no file is at fault:
    /// an artifact another run owes never had one.
    pub fn path(&self) -> Option<&str> {
        match self {
            ArtifactFailure::File { path, .. } => Some(path),
            ArtifactFailure::Content(report) => Some(&report.document.path),
            ArtifactFailure::Unheld { .. } => None,
        }
    }

    /// The report, when the failure is about content — the form a
    /// receipt counts and a diagnostic is rendered from.
    pub fn report(&self) -> Option<&Report> {
        match self {
            ArtifactFailure::File { .. } | ArtifactFailure::Unheld { .. } => None,
            ArtifactFailure::Content(report) => Some(report),
        }
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
            // Headed by the run rather than by a path, because the run
            // is what a reader has to go look at; the remedy holds for
            // both ends of a composition, since either the run produces
            // it or nothing here may ask for it.
            ArtifactFailure::Unheld {
                run,
                producer,
                artifact,
            } => {
                let asked = match producer {
                    Some(node) => format!("node `{node}` of run `{run}`"),
                    None => format!("run `{run}`"),
                };
                f.write_str(&crate::text::problems(
                    asked,
                    &[format!(
                        "holds no {} — produce it there, or stop declaring it here",
                        artifact.label()
                    )],
                ))
            }
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
