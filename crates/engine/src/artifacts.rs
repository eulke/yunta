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

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use yunta_core::diagnostic::{ArtifactFailure, FileProblem, Report};
use yunta_core::events::Finding;
use yunta_core::shape::read;
use yunta_core::FindingsFile;
use yunta_core::NodeId;
use yunta_core::{
    sha256_hex, ArtifactKind, ArtifactSpec, ContentHash, Ledger, Node, Question, QuestionsFile,
};

/// The run directory's own name for where artifacts live. Every path
/// this module produces is relative to the run directory and starts
/// here, which is the shape the log, the diagnostics and the run
/// contract all use.
const ARTIFACTS_DIR: &str = "artifacts";

/// What the engine appends to a `questions` artifact's name when it
/// records the answers beside it.
pub(crate) const ANSWERS_SUFFIX: &str = ".answers.yaml";

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
        match verify_one(&node.id, spec, run_dir, max_bytes) {
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
/// One declared artifact's whole story: the file, its size, and — when the
/// node declared a `kind:` — what it says.
///
/// The node's close and the session's own `yunta_check_artifact` are its
/// two callers, and that is the point: a verdict a session can ask for
/// while it can still act, and the verdict that actually decides the node,
/// have to be the same code or the first one teaches false confidence.
pub(crate) fn verify_one(
    node: &NodeId,
    spec: &ArtifactSpec,
    run_dir: &Path,
    max_bytes: Option<u64>,
) -> Result<VerifiedArtifact, ArtifactFailure> {
    let (name, kind) = match spec {
        ArtifactSpec::Plain(name) => (name, None),
        ArtifactSpec::Typed { name, kind } => (name, Some(*kind)),
    };

    let relative = Path::new(ARTIFACTS_DIR).join(name);
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
    node: &NodeId,
    full_path: &Path,
    path: &str,
    max_bytes: Option<u64>,
) -> Result<Vec<u8>, ArtifactFailure> {
    let about = |problem: FileProblem| ArtifactFailure::file(path, problem);
    let bytes = match std::fs::read(full_path) {
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

/// What the run's `artifacts/` directory held before a node's session
/// ran, so the node's close can tell what that session wrote there.
///
/// Every node of a run writes into that one directory, and a CLI grants
/// writes by directory rather than by file — so a node that declares an
/// artifact can reach every other node's. The worktree has the same
/// shape and the run answers it the same way: the session may write,
/// and the close audits what it wrote against what the node declared.
pub(crate) struct ArtifactsSnapshot(BTreeMap<PathBuf, ContentHash>);

impl ArtifactsSnapshot {
    /// Reads the directory as it stands. A run whose `artifacts/` has
    /// not been created yet snapshots as empty rather than failing:
    /// nothing there is nothing to protect.
    pub(crate) fn take(run_dir: &Path) -> std::io::Result<Self> {
        let mut held = BTreeMap::new();
        collect(&run_dir.join(ARTIFACTS_DIR), &mut |path, bytes| {
            held.insert(path, sha256_hex(bytes));
        })?;
        Ok(Self(held))
    }

    /// The files under `artifacts/` this node changed, added or removed
    /// that it never declared it produces, each relative to the run
    /// directory and named the way the log names an artifact.
    ///
    /// `declared` is the node's rendered artifact names: writing those
    /// is the node doing its job, and rewriting one across a repair is
    /// the repair doing its job.
    pub(crate) fn undeclared_writes(
        &self,
        run_dir: &Path,
        declared: &[String],
    ) -> std::io::Result<Vec<PathBuf>> {
        let owned: BTreeSet<PathBuf> = declared
            .iter()
            .map(|name| Path::new(ARTIFACTS_DIR).join(name))
            .collect();
        // The engine answers a `questions` artifact by writing beside
        // it, and the nodes of a parallel group interleave — so that
        // file can land while a sibling's session is open, and it is
        // the engine's write, never the sibling's.
        let engine_written = |path: &Path| path.to_string_lossy().ends_with(ANSWERS_SUFFIX);
        let mut now = BTreeMap::new();
        collect(&run_dir.join(ARTIFACTS_DIR), &mut |path, bytes| {
            now.insert(path, sha256_hex(bytes));
        })?;
        let touched = now
            .iter()
            .filter(|(path, hash)| self.0.get(*path) != Some(*hash))
            .map(|(path, _)| path.clone());
        let removed = self.0.keys().filter(|path| !now.contains_key(*path));
        Ok(touched
            .chain(removed.cloned())
            .filter(|path| !owned.contains(path) && !engine_written(path))
            .collect())
    }
}

/// Hands every file under `dir` to `visit`, by its path relative to
/// `dir`'s parent — `artifacts/<name>`, the shape the log and every
/// diagnostic already use. An artifact name may nest, so this walks.
fn collect(dir: &Path, visit: &mut impl FnMut(PathBuf, &[u8])) -> std::io::Result<()> {
    fn walk(
        root: &Path,
        dir: &Path,
        visit: &mut impl FnMut(PathBuf, &[u8]),
    ) -> std::io::Result<()> {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                walk(root, &path, visit)?;
                continue;
            }
            let relative = match path.strip_prefix(root) {
                Ok(relative) => Path::new(ARTIFACTS_DIR).join(relative),
                Err(_) => continue,
            };
            visit(relative, &std::fs::read(&path)?);
        }
        Ok(())
    }
    walk(dir, dir, visit)
}
