//! How an artifact enters a run: what a node's close has to answer for,
//! and what its declaration turns the bytes into.
//!
//! When a node finishes, the run has to answer for everything it
//! declared under `artifacts.produces` — no matter what the agent
//! reported. A file the node wrote must exist and be non-empty under
//! that node's own staging directory; a document the run already holds
//! is answered by the log, never by a file. Opaque artifacts are
//! verified by existence and content hash only, never by format.
//! `tasks`, `findings` and `questions` are the interpreted kinds: each
//! is read through the one door that names every problem at once
//! ([`yunta_core::shape::read`], which runs the document's own rules),
//! and the parsed result handed back to the caller — `TasksFile` for
//! `task_registered`, `Finding`s for `finding_posted`, `Question`s so
//! `node_exec.rs` can pause the run instead of finishing the node.
//!
//! What the run then *stores* for a verified artifact is not read here:
//! an interpreted document is re-rendered from what it parsed as, and
//! that is [`super::canonical`]'s subject, along with the documents that
//! never were a file at all.
//!
//! Every failure here is an [`ArtifactFailure`]: a problem with the file
//! (never produced, empty, past the declared ceiling, refused by the
//! filesystem), a document the node ended owing, or a document whose
//! content is not what its kind declares.
use std::path::{Path, PathBuf};

use yunta_core::diagnostic::{ArtifactFailure, FileProblem, Report};
use yunta_core::events::{ArtifactId, Finding, StoredEvent};
use yunta_core::shape::read;
use yunta_core::FindingsFile;
use yunta_core::NodeId;
use yunta_core::{
    sha256_hex, ArtifactKind, ArtifactSpec, ContentHash, Node, Question, QuestionsFile, TasksFile,
};

use super::RunArtifacts;

/// One declared artifact that passed verification: what it is, the file
/// it was read from, its bytes and their hash, and whatever its kind
/// turned those bytes into.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedArtifact {
    /// What the artifact is, which is what the run's log answers by.
    pub artifact: ArtifactId,
    /// Relative to the run directory: where the artifact was read from
    /// or written to — a node's own staging for a file on its way in,
    /// the `artifacts/` view for one the run already holds.
    pub path: PathBuf,
    /// The bytes as they were read or written. What the run stores is
    /// their canonical rendering, which differs whenever a node wrote an
    /// interpreted document in its own spelling — so the hash of what
    /// the run holds comes from [`accept`](super::accept), never from
    /// here.
    pub bytes: Vec<u8>,
    /// The hash of the file this node staged, for the one construction
    /// that read a file. `None` for a document that was never one: an
    /// artifact the run already holds, a document a session submitted,
    /// one the engine derived.
    pub staged: Option<StagedHash>,
    pub content: ArtifactContent,
}

/// The hash of a file as a node staged it.
///
/// Never the hash the run's own store answers for. What the run stores
/// for an interpreted document is the canonical rendering of what those
/// bytes parsed as, so a node that wrote a tasks document in its own
/// spelling staged one hash and the run holds another — and a reader
/// that compared them would call one of the two wrong. The store's
/// answer is [`accept`](super::accept)'s to give; this one says what
/// was on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedHash(ContentHash);

impl StagedHash {
    /// The hash of the bytes a node staged.
    pub fn of(bytes: &[u8]) -> Self {
        StagedHash(sha256_hex(bytes))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl std::fmt::Display for StagedHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a declared `kind:` turned the bytes into. `Opaque` is what "the
/// engine assumes no format" looks like from here: there is no third
/// state where a kind was declared and nothing was parsed.
#[derive(Debug, Clone, PartialEq)]
pub enum ArtifactContent {
    Opaque,
    Tasks(TasksFile),
    Findings(Vec<Finding>),
    Questions(Vec<Question>),
    Answers(Vec<yunta_core::Answer>),
}

impl ArtifactContent {
    /// The kind whose shape produced this content — `None` for an
    /// opaque artifact, which is what the engine records for a file it
    /// never interprets.
    pub fn kind(&self) -> Option<ArtifactKind> {
        match self {
            ArtifactContent::Opaque => None,
            ArtifactContent::Tasks(_) => Some(ArtifactKind::Tasks),
            ArtifactContent::Findings(_) => Some(ArtifactKind::Findings),
            ArtifactContent::Questions(_) => Some(ArtifactKind::Questions),
            ArtifactContent::Answers(_) => Some(ArtifactKind::Answers),
        }
    }
}

/// Verifies every artifact a node declared, collecting every violation
/// instead of stopping at the first — the node fails once with the whole
/// picture, not once per missing artifact.
///
/// Where each answer comes from is the node's own kind: a document this
/// node handed over or the engine derived is already a fact of `events`,
/// read back from the object store, and no file stands in for it;
/// everything else is a file the node wrote in its staging, and this is
/// where that file enters the run. `max_bytes` is
/// `limits.max_artifact_bytes` when declared — `None` means unbounded,
/// and it guards a file on its way in, the one thing already settled for
/// what the run holds.
pub async fn close_artifacts(
    node: &Node,
    run_dir: &Path,
    events: &[StoredEvent],
    max_bytes: Option<u64>,
) -> Result<Vec<VerifiedArtifact>, Vec<ArtifactFailure>> {
    let Some(artifacts) = &node.artifacts else {
        return Ok(Vec::new());
    };
    let held = RunArtifacts::of(run_dir, events);

    let mut verified = Vec::new();
    let mut failures = Vec::new();
    for spec in &artifacts.produces {
        let answer = if super::answered_by_the_log(&node.kind, spec.kind()) {
            held_document(&node.id, spec, &held).await
        } else {
            verify_one(&node.id, spec, run_dir, max_bytes).await
        };
        match answer {
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

/// The artifact the run already holds for one declaration of `node`,
/// read as the kind that declaration gives it.
///
/// Nothing on disk is consulted: the acceptance standing on the run's
/// log is what says the artifact exists, and its bytes come from the
/// object store. A run that holds none of it was handed none — a
/// document arrives through the submission tool or is derived from what
/// the node posted, so no file could stand in for one that never
/// arrived, and the node is owed exactly what it declared.
///
/// The node's close and a session's own `yunta_check_artifact` both read
/// here, which is what keeps the verdict a session can still act on and
/// the verdict that decides the node one answer.
pub(crate) async fn held_document(
    node: &NodeId,
    spec: &ArtifactSpec,
    held: &super::RunArtifacts<'_>,
) -> Result<VerifiedArtifact, ArtifactFailure> {
    let artifact = ArtifactId::from(spec);
    let Some(found) = held.held(&artifact, Some(node)) else {
        return Err(ArtifactFailure::Undelivered {
            node: node.clone(),
            artifact,
        });
    };
    let bytes = held.bytes(found).await.map_err(|source| {
        ArtifactFailure::file(
            view_path(node, &artifact),
            FileProblem::Unreadable {
                detail: source.to_string(),
            },
        )
    })?;
    interpreted(Some(node), spec, &bytes)
}

/// One declared artifact's whole story: the file, its size, and — when the
/// node declared a `kind:` — what it says.
///
/// The node's close and the session's own `yunta_check_artifact` are its
/// two callers, and that is the point: a verdict a session can ask for
/// while it can still act, and the verdict that actually decides the node,
/// have to be the same code or the first one teaches false confidence.
pub(crate) async fn verify_one(
    node: &NodeId,
    spec: &ArtifactSpec,
    run_dir: &Path,
    max_bytes: Option<u64>,
) -> Result<VerifiedArtifact, ArtifactFailure> {
    let artifact = ArtifactId::from(spec);
    // A file on its way in sits under the name the node writes it as,
    // which for a document the engine reads is that document's own name
    // for itself.
    let relative = crate::run_dir::staged_path(node, &artifact.view_name());
    let path = relative.display().to_string();

    let bytes = read_file(node, &run_dir.join(&relative), &path, max_bytes).await?;
    let content = interpret(spec.kind(), &bytes, &path).map_err(ArtifactFailure::Content)?;

    Ok(VerifiedArtifact {
        artifact,
        path: relative,
        staged: Some(StagedHash::of(&bytes)),
        bytes,
        content,
    })
}

/// One artifact the run already holds, read as the kind its declaration
/// gives it.
///
/// The bytes come from the object store, so there is no file to find, no
/// emptiness to catch and no ceiling to enforce: those are questions
/// about a file on its way in, already answered before the run accepted
/// it. What is left is the reading, and it is the reading the close does.
pub(crate) fn interpreted(
    node: Option<&NodeId>,
    spec: &ArtifactSpec,
    bytes: &[u8],
) -> Result<VerifiedArtifact, ArtifactFailure> {
    let artifact = ArtifactId::from(spec);
    let path = super::store::view_path(node, &artifact.view_name());
    let content = interpret(spec.kind(), bytes, &path.display().to_string())
        .map_err(ArtifactFailure::Content)?;
    Ok(VerifiedArtifact {
        artifact,
        path,
        staged: None,
        bytes: bytes.to_vec(),
        content,
    })
}

/// Where `node`'s view of `artifact` sits, as a diagnostic names it.
pub(super) fn view_path(node: &NodeId, artifact: &ArtifactId) -> String {
    super::store::view_path(Some(node), &artifact.view_name())
        .display()
        .to_string()
}

/// The file itself, before anything inside it is read: it exists, it has
/// content, and it is within the declared guard. Nothing a rewrite of
/// the content reaches, which is why each answer here is a
/// [`FileProblem`] rather than a diagnostic about a document.
async fn read_file(
    node: &NodeId,
    full_path: &Path,
    path: &str,
    max_bytes: Option<u64>,
) -> Result<Vec<u8>, ArtifactFailure> {
    let about = |problem: FileProblem| ArtifactFailure::file(path, problem);
    let bytes = match tokio::fs::read(full_path).await {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(about(FileProblem::Missing { node: node.clone() }))
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
pub(super) fn interpret(
    kind: Option<ArtifactKind>,
    bytes: &[u8],
    path: &str,
) -> Result<ArtifactContent, Report> {
    Ok(match kind {
        None => ArtifactContent::Opaque,
        Some(ArtifactKind::Tasks) => ArtifactContent::Tasks(read::<TasksFile>(bytes, path)?),
        Some(ArtifactKind::Findings) => {
            let file = read::<FindingsFile>(bytes, path)?;
            ArtifactContent::Findings(file.findings.into_iter().map(Finding::from).collect())
        }
        Some(ArtifactKind::Questions) => {
            let file = read::<QuestionsFile>(bytes, path)?;
            ArtifactContent::Questions(file.questions)
        }
        Some(ArtifactKind::Answers) => {
            let file = read::<yunta_core::AnswersFile>(bytes, path)?;
            ArtifactContent::Answers(file.answers)
        }
    })
}
