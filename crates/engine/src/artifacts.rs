//! Artifact verification at node close.
//!
//! When a node finishes, everything it declared under `artifacts.produces`
//! must exist and be non-empty under the run's `artifacts/` directory —
//! no matter what the agent reported. Opaque artifacts are verified
//! by existence and content hash only, never by format. `task-ledger`,
//! `findings` and `questions` are the interpreted
//! kinds: each is parsed and validated with every violation reported
//! together, and the parsed result handed back to the caller — `Ledger`
//! for `task_registered`, `Finding`s for `finding_posted`, `Question`s so
//! `node_exec.rs` can pause the run instead of finishing the node.

use std::path::{Path, PathBuf};

use thiserror::Error;
use yunta_core::events::Finding;
use yunta_core::FindingsFile;
use yunta_core::{
    sha256_hex, ArtifactKind, ArtifactSpec, Ledger, Node, NodeId, Question, QuestionsFile,
};

use crate::findings::FindingsError;
use crate::ledger::LedgerError;
use crate::questions::QuestionsError;

#[derive(Debug, Error)]
pub enum ArtifactError {
    #[error("node `{node}` declared artifact `{name}` but never produced it")]
    Missing { node: NodeId, name: String },

    #[error(
        "node `{node}` produced artifact `{name}` empty — a declared artifact must have content"
    )]
    Empty { node: NodeId, name: String },

    /// Guard against accidents: a runaway artifact fails the node with
    /// both numbers on the table, never a truncation.
    #[error(
        "node `{node}` produced artifact `{name}` at {bytes} bytes — \
         `limits.max_artifact_bytes` is {max_bytes}"
    )]
    Oversized {
        node: NodeId,
        name: String,
        bytes: u64,
        max_bytes: u64,
    },

    #[error("node `{node}`: artifact `{path}` exists but cannot be read")]
    Unreadable {
        node: NodeId,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("node `{node}`: task ledger `{name}` is not valid YAML: {detail}")]
    MalformedLedger {
        node: NodeId,
        name: String,
        detail: String,
    },

    #[error("node `{node}`: task ledger `{name}` failed validation with {} error(s)", errors.len())]
    InvalidLedger {
        node: NodeId,
        name: String,
        errors: Vec<LedgerError>,
    },

    #[error("node `{node}`: findings `{name}` is not valid YAML: {detail}")]
    MalformedFindings {
        node: NodeId,
        name: String,
        detail: String,
    },

    #[error("node `{node}`: findings `{name}` failed validation with {} error(s)", errors.len())]
    InvalidFindings {
        node: NodeId,
        name: String,
        errors: Vec<FindingsError>,
    },

    #[error("node `{node}`: questions `{name}` is not valid YAML: {detail}")]
    MalformedQuestions {
        node: NodeId,
        name: String,
        detail: String,
    },

    #[error("node `{node}`: questions `{name}` failed validation with {} error(s)", errors.len())]
    InvalidQuestions {
        node: NodeId,
        name: String,
        errors: Vec<QuestionsError>,
    },
}

/// One declared artifact that passed verification — the data
/// `artifact_written` needs (run-dir-relative path + content hash), plus
/// the parsed ledger or findings when the artifact carries one of those
/// interpreted kinds.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedArtifact {
    pub name: String,
    /// Relative to the run directory (`artifacts/<name>`).
    pub path: PathBuf,
    pub content_hash: String,
    /// The declared `kind:`, if any — carried onto `artifact_written`
    /// so replay can recognize interpreted artifacts by type.
    pub kind: Option<ArtifactKind>,
    pub ledger: Option<Ledger>,
    pub findings: Option<Vec<Finding>>,
    pub questions: Option<Vec<Question>>,
}

/// Verifies every artifact a node declared, collecting every violation
/// instead of stopping at the first — the node fails once with the whole
/// picture, not once per missing file. `max_bytes` is
/// `limits.max_artifact_bytes` when declared — `None` means unbounded.
pub fn close_artifacts(
    node: &Node,
    run_dir: &Path,
    max_bytes: Option<u64>,
) -> Result<Vec<VerifiedArtifact>, Vec<ArtifactError>> {
    let Some(artifacts) = &node.artifacts else {
        return Ok(Vec::new());
    };

    let mut verified = Vec::new();
    let mut errors = Vec::new();

    for spec in &artifacts.produces {
        let (name, kind) = match spec {
            ArtifactSpec::Plain(name) => (name, None),
            ArtifactSpec::Typed { name, kind } => (name, Some(kind)),
        };

        let relative = Path::new("artifacts").join(name);
        let full_path = run_dir.join(&relative);

        let bytes = match std::fs::read(&full_path) {
            Ok(bytes) => bytes,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                errors.push(ArtifactError::Missing {
                    node: node.id.clone(),
                    name: name.clone(),
                });
                continue;
            }
            Err(source) => {
                errors.push(ArtifactError::Unreadable {
                    node: node.id.clone(),
                    path: full_path,
                    source,
                });
                continue;
            }
        };

        if bytes.is_empty() {
            errors.push(ArtifactError::Empty {
                node: node.id.clone(),
                name: name.clone(),
            });
            continue;
        }

        if let Some(max_bytes) = max_bytes {
            if bytes.len() as u64 > max_bytes {
                errors.push(ArtifactError::Oversized {
                    node: node.id.clone(),
                    name: name.clone(),
                    bytes: bytes.len() as u64,
                    max_bytes,
                });
                continue;
            }
        }

        let mut ledger = None;
        let mut findings = None;
        let mut questions = None;
        match kind {
            Some(ArtifactKind::TaskLedger) => match parse_ledger(&node.id, name, &bytes) {
                Ok(parsed) => ledger = Some(parsed),
                Err(error) => {
                    errors.push(error);
                    continue;
                }
            },
            Some(ArtifactKind::Findings) => match parse_findings(&node.id, name, &bytes) {
                Ok(parsed) => findings = Some(parsed),
                Err(error) => {
                    errors.push(error);
                    continue;
                }
            },
            Some(ArtifactKind::Questions) => match parse_questions(&node.id, name, &bytes) {
                Ok(parsed) => questions = Some(parsed),
                Err(error) => {
                    errors.push(error);
                    continue;
                }
            },
            None => {}
        };

        verified.push(VerifiedArtifact {
            name: name.clone(),
            path: relative,
            content_hash: sha256_hex(&bytes),
            kind: kind.cloned(),
            ledger,
            findings,
            questions,
        });
    }

    if errors.is_empty() {
        Ok(verified)
    } else {
        Err(errors)
    }
}

fn parse_findings(node: &NodeId, name: &str, bytes: &[u8]) -> Result<Vec<Finding>, ArtifactError> {
    let file: FindingsFile =
        yunta_core::yaml::parse_bytes(bytes).map_err(|e| ArtifactError::MalformedFindings {
            node: node.clone(),
            name: name.to_string(),
            detail: e.to_string(),
        })?;

    let violations = crate::findings::register(&file);
    if violations.is_empty() {
        Ok(file.findings.into_iter().map(Finding::from).collect())
    } else {
        Err(ArtifactError::InvalidFindings {
            node: node.clone(),
            name: name.to_string(),
            errors: violations,
        })
    }
}

fn parse_questions(
    node: &NodeId,
    name: &str,
    bytes: &[u8],
) -> Result<Vec<Question>, ArtifactError> {
    let file: QuestionsFile =
        yunta_core::yaml::parse_bytes(bytes).map_err(|e| ArtifactError::MalformedQuestions {
            node: node.clone(),
            name: name.to_string(),
            detail: e.to_string(),
        })?;

    let violations = crate::questions::register(&file);
    if violations.is_empty() {
        Ok(file.questions)
    } else {
        Err(ArtifactError::InvalidQuestions {
            node: node.clone(),
            name: name.to_string(),
            errors: violations,
        })
    }
}

fn parse_ledger(node: &NodeId, name: &str, bytes: &[u8]) -> Result<Ledger, ArtifactError> {
    let ledger: Ledger =
        yunta_core::yaml::parse_bytes(bytes).map_err(|e| ArtifactError::MalformedLedger {
            node: node.clone(),
            name: name.to_string(),
            detail: e.to_string(),
        })?;

    let violations = crate::ledger::register(&ledger);
    if violations.is_empty() {
        Ok(ledger)
    } else {
        Err(ArtifactError::InvalidLedger {
            node: node.clone(),
            name: name.to_string(),
            errors: violations,
        })
    }
}
