//! Why one declared artifact did not close.
//!
//! A file that was never written, a document nobody handed over, a file
//! whose content is wrong and an artifact another run owes are four
//! failures, and every surface reads them differently: one says the node
//! wrote no file, one says the node ended owing a document, one says what
//! the document it produced got wrong, and one says nothing this run
//! does reaches the artifact at all. Four variants make that a fact the
//! compiler carries rather than a predicate somebody has to remember to
//! call.
//!
//! Only the first names a path, because it is the only one a close went
//! looking on disk for. The rest answer about something the log states,
//! and a path invented for them would send a reader to a file that was
//! never going to be there.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::Report;
use crate::events::ArtifactId;
use crate::{NodeId, RunId};

/// Why one declared artifact did not close.
///
/// The variants answer the one question that decides what is worth
/// doing next: rewriting the content reaches [`Content`](Self::Content),
/// nothing this node writes reaches [`File`](Self::File) or
/// [`Undelivered`](Self::Undelivered), and nothing this run does at all
/// reaches [`Unheld`](Self::Unheld), because the artifact is another
/// run's to produce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "failure", rename_all = "kebab-case")]
pub enum ArtifactFailure {
    /// The file itself. No rewrite of its content reaches this.
    File { path: String, problem: FileProblem },
    /// Declared by a node the run's own log answers for, and never
    /// handed over: the node ended without the document it owes.
    ///
    /// No path, for the same reason [`Unheld`](Self::Unheld) has none.
    /// Such a document never is a file on its way in — a session hands
    /// it over through its submission tool, the engine derives it from
    /// what the node posted — so the close opened nothing and there is
    /// no file to name.
    Undelivered {
        /// The node that declared it and ended owing it.
        node: NodeId,
        /// What the node owes: the identity, which is what a log
        /// answers by.
        artifact: ArtifactId,
    },
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
            ArtifactFailure::Undelivered { .. } => Some("artifact-undelivered"),
            ArtifactFailure::Unheld { .. } => Some("artifact-unheld"),
            ArtifactFailure::Content(_) => None,
        }
    }

    /// Where a reader opens the file. `None` when no file is at fault:
    /// a document nobody handed over and an artifact another run owes
    /// never had one.
    pub fn path(&self) -> Option<&str> {
        match self {
            ArtifactFailure::File { path, .. } => Some(path),
            ArtifactFailure::Content(report) => Some(&report.document.path),
            ArtifactFailure::Undelivered { .. } | ArtifactFailure::Unheld { .. } => None,
        }
    }

    /// The report, when the failure is about content — the form a
    /// receipt counts and a diagnostic is rendered from.
    pub fn report(&self) -> Option<&Report> {
        match self {
            ArtifactFailure::File { .. }
            | ArtifactFailure::Undelivered { .. }
            | ArtifactFailure::Unheld { .. } => None,
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
            // Headed by the node rather than by a path: the node is
            // what a reader has to go instruct, and the remedy is the
            // same shape as `Unheld`'s, since either the node produces
            // it or nothing here may declare it.
            ArtifactFailure::Undelivered { node, artifact } => f.write_str(&crate::text::problems(
                format!("node `{node}`"),
                &[format!(
                    "handed over no {} — produce it before the node ends, or stop declaring it here",
                    artifact.label()
                )],
            )),
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
