//! Why a node failed, as data.
//!
//! The one payload field that is not a plain value: a node fails either
//! with a sentence the engine states, or with declared artifacts that
//! did not close, each saying what went wrong with it. Keeping both in
//! one type is what lets the log record the facts and every surface
//! produce its own prose from them, instead of the engine writing prose
//! once and three surfaces taking it apart again.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::diagnostic::{ArtifactFailure, Report};
use crate::glob::{InvalidScopeGlob, ScopeGlob};
use crate::ids::AdapterId;

/// How many stderr lines a session keeps for its exit (D180): enough to
/// read a CLI's startup error, and not enough for a whole session log to
/// ride along in an event.
pub const STDERR_TAIL_LINES: usize = 20;

/// How the process ended: the status it exited with, or the signal that
/// ended it.
///
/// A closed union rather than two optionals, so a process that says
/// neither is not representable. A stored `type` this build does not
/// know reads back as [`SessionEnd::Unknown`] — the tolerance everything
/// persisted here gives a reader older than its writer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEnd {
    Code {
        code: i32,
    },
    Signal {
        signal: i32,
    },
    #[serde(other)]
    Unknown,
}

/// What a process left behind: how it ended, and the last lines it wrote
/// to stderr, redacted of every value its environment carried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SessionExit {
    pub end: SessionEnd,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stderr_tail: Vec<String>,
}

/// A session that ended without ever reporting a terminal event: whose
/// it was, and how its process went, when it had one of its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SessionDeath {
    pub adapter: AdapterId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<SessionExit>,
}

/// Why a node failed.
///
/// Untagged, with `Message` last: a payload carrying `artifacts:` reads
/// as [`Failure::Artifacts`], one carrying `died:` as
/// [`Failure::SessionDied`], one carrying `outside_scope:` as
/// [`Failure::ScopeViolated`], and a log written before failures were
/// data carries `outcome:` alone and reads back as
/// [`Failure::Message`]. That tolerance is the rule for what is
/// persisted and versioned, and it is why no reader needs to know which
/// version wrote the line it is looking at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum Failure {
    /// Declared artifacts that did not close, each saying what went
    /// wrong with it: the file, the content of the document, or the run
    /// that owes the artifact and holds none of it.
    Artifacts { artifacts: Vec<ArtifactFailure> },
    /// The session the node was working in ended without a terminal
    /// event, and how its process went.
    SessionDied { died: SessionDeath },
    /// The node's diff reached paths no glob of its scope allows. Named
    /// one by one, because what a person does next — widen the scope,
    /// or change those files in the run's tree — is about exactly them.
    ScopeViolated { outside_scope: Vec<PathBuf> },
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

    /// A session of `adapter` that ended without a terminal event.
    /// `exit` is what its process left behind, absent for a session
    /// with no process of its own.
    pub fn session_died(adapter: AdapterId, exit: Option<SessionExit>) -> Self {
        Failure::SessionDied {
            died: SessionDeath { adapter, exit },
        }
    }

    /// A diff that reached `outside_scope`, which the node's scope does
    /// not allow.
    pub fn scope_violated(outside_scope: Vec<PathBuf>) -> Self {
        Failure::ScopeViolated { outside_scope }
    }

    /// The paths the node wrote outside its scope. Empty for a failure
    /// that is not about scope.
    pub fn outside_scope(&self) -> &[PathBuf] {
        match self {
            Failure::ScopeViolated { outside_scope } => outside_scope,
            Failure::Artifacts { .. } | Failure::SessionDied { .. } | Failure::Message { .. } => {
                &[]
            }
        }
    }

    /// Whether a wider scope is what this failure needs — what makes a
    /// grant a way forward rather than a guess.
    pub fn wants_scope(&self) -> bool {
        !self.outside_scope().is_empty()
    }

    /// What a grant would add to the node's scope for the work behind
    /// this failure to stand: each path it wrote outside, exactly. Empty
    /// for a failure a wider scope would not change.
    pub fn scope_wanted(&self) -> Result<Vec<ScopeGlob>, InvalidScopeGlob> {
        self.outside_scope()
            .iter()
            .map(|path| ScopeGlob::exact(path))
            .collect()
    }

    /// [`scope_wanted`](Self::scope_wanted) as a sentence lists it: the
    /// paths as a person reads them, comma-separated.
    pub fn scope_wanted_listed(&self) -> String {
        self.outside_scope()
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Every report behind this failure, each carrying the document it
    /// is about. What a diagnostic is rendered from; a failure whose
    /// artifacts name no document yields none.
    pub fn reports(&self) -> impl Iterator<Item = &Report> {
        self.failures().filter_map(ArtifactFailure::report)
    }

    /// Every declared artifact that did not close. Empty for a failure
    /// that has nothing to do with artifacts.
    pub fn failures(&self) -> impl Iterator<Item = &ArtifactFailure> {
        match self {
            Failure::Artifacts { artifacts } => artifacts.iter(),
            // A dead session names no artifact, and neither does a
            // scope or a sentence: the count of documents that did not
            // close is about documents this node declared.
            Failure::SessionDied { .. }
            | Failure::ScopeViolated { .. }
            | Failure::Message { .. } => [].iter(),
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
            Failure::SessionDied { died } => write!(f, "{died}"),
            Failure::ScopeViolated { outside_scope } => {
                write!(
                    f,
                    "scope violated: {} file(s) outside the declared globs — ",
                    outside_scope.len()
                )?;
                for (position, path) in outside_scope.iter().enumerate() {
                    if position > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{}", path.display())?;
                }
                Ok(())
            }
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

impl fmt::Display for SessionDeath {
    /// Every shape the type admits, the absence included: a session with
    /// no process of its own has nothing to report about one.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(exit) = &self.exit else {
            return write!(
                f,
                "session `{}` ended without a terminal event",
                self.adapter
            );
        };
        write!(
            f,
            "session `{}` {} before any terminal event",
            self.adapter, exit.end
        )?;
        match exit.stderr_tail.last() {
            Some(last) => write!(f, " — {last}"),
            None => Ok(()),
        }
    }
}

impl fmt::Display for SessionEnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionEnd::Code { code } => write!(f, "exited with code {code}"),
            SessionEnd::Signal { signal } => write!(f, "was killed by signal {signal}"),
            SessionEnd::Unknown => f.write_str("ended in a way this build does not know"),
        }
    }
}
