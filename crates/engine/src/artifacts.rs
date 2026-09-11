//! Artifact verification at node close.
//!
//! When a node finishes, everything it declared under
//! `artifacts.produces` must exist and be non-empty under the run's
//! `artifacts/` directory — no matter what the agent reported. Opaque
//! artifacts are verified by existence and content hash only, never by
//! format. `task-ledger`, `findings` and `questions` are the interpreted
//! kinds: each is read through the one door that names every problem at
//! once ([`yunta_core::shape::read`], which runs the document's own
//! rules), and the parsed result handed back to the caller — `Ledger`
//! for `task_registered`, `Finding`s for `finding_posted`, `Question`s
//! so `node_exec.rs` can pause the run instead of finishing the node.
//!
//! Every failure here is an [`ArtifactFailure`], and its two variants
//! are the distinction the repair cycle turns on: a problem with the
//! file (never produced, empty, past the declared ceiling, refused by
//! the filesystem) is not one a rewrite reaches, while a document whose
//! content is not what its kind declares is exactly what writing the
//! file again fixes.

use std::path::{Path, PathBuf};

use yunta_core::diagnostic::{ArtifactFailure, FileProblem, Report};
use yunta_core::events::Finding;
use yunta_core::shape::read;
use yunta_core::FindingsFile;
use yunta_core::{
    sha256_hex, ArtifactKind, ArtifactSpec, ContentHash, Ledger, Node, Question, QuestionsFile,
};

/// One declared artifact that passed verification — the data
/// `artifact_written` needs (run-dir-relative path + content hash), plus
/// whatever its declared `kind:` turned the bytes into.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedArtifact {
    pub name: String,
    /// Relative to the run directory (`artifacts/<name>`).
    pub path: PathBuf,
    pub content_hash: ContentHash,
    pub content: ArtifactContent,
}

/// What a declared `kind:` turned the bytes into. `Opaque` is what "the
/// engine assumes no format" looks like from here: there is no third
/// state where a kind was declared and nothing was parsed.
#[derive(Debug, Clone, PartialEq)]
pub enum ArtifactContent {
    Opaque,
    TaskLedger(Ledger),
    Findings(Vec<Finding>),
    Questions(Vec<Question>),
}

impl ArtifactContent {
    /// The kind whose shape produced this content — `None` for an
    /// opaque artifact, which is what `artifact_written` records for a
    /// file the engine never interprets.
    pub fn kind(&self) -> Option<ArtifactKind> {
        match self {
            ArtifactContent::Opaque => None,
            ArtifactContent::TaskLedger(_) => Some(ArtifactKind::TaskLedger),
            ArtifactContent::Findings(_) => Some(ArtifactKind::Findings),
            ArtifactContent::Questions(_) => Some(ArtifactKind::Questions),
        }
    }
}

/// Verifies every artifact a node declared, collecting every violation
/// instead of stopping at the first — the node fails once with the whole
/// picture, not once per missing file. `max_bytes` is
/// `limits.max_artifact_bytes` when declared — `None` means unbounded.
pub fn close_artifacts(
    node: &Node,
    run_dir: &Path,
    max_bytes: Option<u64>,
) -> Result<Vec<VerifiedArtifact>, Vec<ArtifactFailure>> {
    let Some(artifacts) = &node.artifacts else {
        return Ok(Vec::new());
    };

    let mut verified = Vec::new();
    let mut failures = Vec::new();
    for spec in &artifacts.produces {
        match verify_one(node, spec, run_dir, max_bytes) {
            Ok(artifact) => verified.push(artifact),
            Err(failure) => failures.push(failure),
        }
    }

    if failures.is_empty() {
        Ok(verified)
    } else {
        Err(failures)
    }
}

/// One declared artifact's whole story: the file, its size, and — when
/// the node declared a `kind:` — what it says.
fn verify_one(
    node: &Node,
    spec: &ArtifactSpec,
    run_dir: &Path,
    max_bytes: Option<u64>,
) -> Result<VerifiedArtifact, ArtifactFailure> {
    let (name, kind) = match spec {
        ArtifactSpec::Plain(name) => (name, None),
        ArtifactSpec::Typed { name, kind } => (name, Some(*kind)),
    };

    let relative = Path::new("artifacts").join(name);
    let path = relative.display().to_string();

    let bytes = read_file(node, &run_dir.join(&relative), &path, max_bytes)?;
    let content = interpret(kind, &bytes, &path).map_err(ArtifactFailure::Content)?;

    Ok(VerifiedArtifact {
        name: name.clone(),
        path: relative,
        content_hash: sha256_hex(&bytes),
        content,
    })
}

/// The file itself, before anything inside it is read: it exists, it has
/// content, and it is within the declared guard. Nothing a rewrite of
/// the content reaches, which is why each answer here is a
/// [`FileProblem`] rather than a diagnostic about a document.
fn read_file(
    node: &Node,
    full_path: &Path,
    path: &str,
    max_bytes: Option<u64>,
) -> Result<Vec<u8>, ArtifactFailure> {
    let about = |problem: FileProblem| ArtifactFailure::file(path, problem);
    let bytes = match std::fs::read(full_path) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(about(FileProblem::Missing {
                node: node.id.clone(),
            }))
        }
        Err(source) => {
            return Err(about(FileProblem::Unreadable {
                detail: source.to_string(),
            }))
        }
    };

    if bytes.is_empty() {
        return Err(about(FileProblem::Empty));
    }
    // A runaway artifact fails the node with both numbers on the table,
    // never a truncation.
    if let Some(ceiling) = max_bytes {
        if bytes.len() as u64 > ceiling {
            return Err(about(FileProblem::Oversized {
                bytes: bytes.len() as u64,
                ceiling,
            }));
        }
    }
    Ok(bytes)
}

/// Reads one artifact's bytes as whatever its declared kind says they
/// are. An artifact with no declared kind is [`ArtifactContent::Opaque`]
/// without touching the bytes at all.
fn interpret(
    kind: Option<ArtifactKind>,
    bytes: &[u8],
    path: &str,
) -> Result<ArtifactContent, Report> {
    Ok(match kind {
        None => ArtifactContent::Opaque,
        Some(ArtifactKind::TaskLedger) => ArtifactContent::TaskLedger(read::<Ledger>(bytes, path)?),
        Some(ArtifactKind::Findings) => {
            let file = read::<FindingsFile>(bytes, path)?;
            ArtifactContent::Findings(file.findings.into_iter().map(Finding::from).collect())
        }
        Some(ArtifactKind::Questions) => {
            let file = read::<QuestionsFile>(bytes, path)?;
            ArtifactContent::Questions(file.questions)
        }
    })
}
