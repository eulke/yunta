//! Artifact verification at node close (Contrato §4/§4.1) — **M-0 cut**.
//!
//! When a node finishes, everything it declared under `artifacts.produces`
//! must exist and be non-empty under the run's `artifacts/` directory —
//! no matter what the agent reported (I5). Opaque artifacts are verified
//! by existence and content hash only, never by format. `kind:
//! task-ledger` is the one interpreted kind in M-0: the file is parsed
//! and validated with every violation reported together (spec-ledger §4),
//! and the resulting [`Ledger`] is handed back so the scheduler can emit
//! `task_registered` per task. `findings`/`questions` arrive with their
//! own milestones.

use std::path::{Path, PathBuf};

use thiserror::Error;
use yunta_core::{sha256_hex, ArtifactKind, ArtifactSpec, Ledger, Node, NodeId};

use crate::ledger::LedgerError;

#[derive(Debug, Error)]
pub enum ArtifactError {
    #[error("node `{node}` declared artifact `{name}` but never produced it")]
    Missing { node: NodeId, name: String },

    #[error(
        "node `{node}` produced artifact `{name}` empty — a declared artifact must have content"
    )]
    Empty { node: NodeId, name: String },

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
}

/// One declared artifact that passed verification — the data
/// `artifact_written` needs (run-dir-relative path + content hash), plus
/// the parsed ledger when the artifact is a `task-ledger`.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedArtifact {
    pub name: String,
    /// Relative to the run directory (`artifacts/<name>`).
    pub path: PathBuf,
    pub content_hash: String,
    pub ledger: Option<Ledger>,
}

/// Verifies every artifact a node declared, collecting every violation
/// instead of stopping at the first — the node fails once with the whole
/// picture, not once per missing file.
pub fn close_artifacts(
    node: &Node,
    run_dir: &Path,
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

        let ledger = match kind {
            Some(ArtifactKind::TaskLedger) => match parse_ledger(&node.id, name, &bytes) {
                Ok(ledger) => Some(ledger),
                Err(error) => {
                    errors.push(error);
                    continue;
                }
            },
            None => None,
        };

        verified.push(VerifiedArtifact {
            name: name.clone(),
            path: relative,
            content_hash: sha256_hex(&bytes),
            ledger,
        });
    }

    if errors.is_empty() {
        Ok(verified)
    } else {
        Err(errors)
    }
}

fn parse_ledger(node: &NodeId, name: &str, bytes: &[u8]) -> Result<Ledger, ArtifactError> {
    let ledger: Ledger =
        serde_yaml::from_slice(bytes).map_err(|e| ArtifactError::MalformedLedger {
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
