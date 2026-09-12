//! How an artifact enters a run: verified at a node's close, submitted
//! by a session, or derived by the engine from the log.
//!
//! When a node finishes, everything it declared under
//! `artifacts.produces` must exist and be non-empty under the run's
//! `artifacts/` directory — no matter what the agent reported. Opaque
//! artifacts are verified by existence and content hash only, never by
//! format. `tasks`, `findings` and `questions` are the interpreted
//! kinds: each is read through the one door that names every problem at
//! once ([`yunta_core::shape::read`], which runs the document's own
//! rules), and the parsed result handed back to the caller — `TasksFile`
//! for `task_registered`, `Finding`s for `finding_posted`, `Question`s
//! so `node_exec.rs` can pause the run instead of finishing the node.
//!
//! An interpreted artifact reaches that directory through this module
//! too, and only through it: [`submit`] takes the document a session
//! handed over, validates it against the kind the node declared, and
//! writes the canonical YAML; [`derive_findings`] writes the file a
//! reviewing node's own postings add up to. The close then reads back
//! what was written, through the same door as any other file — so a
//! document is judged once, by the code that judges every document.
//!
//! Whatever the file on disk spells, what the run stores is its
//! canonical rendering: an interpreted document re-rendered from what it
//! parsed as, so an artifact of a kind is the same bytes whoever wrote
//! it.
//!
//! Every failure here is an [`ArtifactFailure`]: a problem with the file
//! (never produced, empty, past the declared ceiling, refused by the
//! filesystem) or a document whose content is not what its kind
//! declares.

use std::path::{Path, PathBuf};

use yunta_core::diagnostic::{ArtifactFailure, FileProblem, Report};
use yunta_core::events::Finding;
use yunta_core::shape::read;
use yunta_core::FindingsFile;
use yunta_core::NodeId;
use yunta_core::{
    sha256_hex, ArtifactKind, ArtifactSpec, ContentHash, Node, Question, QuestionsFile, TasksFile,
    ARTIFACTS_DIR,
};

/// One declared artifact that passed verification: the name it was
/// declared under, the file it was read from, its bytes and their hash,
/// and whatever its declared `kind:` turned those bytes into.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedArtifact {
    pub name: String,
    /// Relative to the run directory (`artifacts/<name>`).
    pub path: PathBuf,
    /// The bytes as they were read or written, which is what
    /// `content_hash` is the hash of. What the run stores is their
    /// canonical rendering, which differs whenever a node wrote an
    /// interpreted document in its own spelling.
    pub bytes: Vec<u8>,
    pub content_hash: ContentHash,
    pub content: ArtifactContent,
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
        bytes,
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
        Some(ArtifactKind::Tasks) => ArtifactContent::Tasks(read::<TasksFile>(bytes, path)?),
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

/// Why a document a session handed over did not become a file.
#[derive(Debug)]
pub(crate) enum SubmitError {
    /// The name is declared without a `kind:` — a file the session
    /// writes itself, with no document for the engine to validate.
    NotInterpreted { name: String },
    /// The name is a findings artifact, whose entries arrive one at a
    /// time and whose file the engine derives at close. Defensive: no
    /// submission tool is offered for the kind.
    Accumulated { name: String },
    /// The document is not what its kind declares.
    Refused(Report),
    /// The canonical file would be larger than the run allows.
    File { path: String, problem: FileProblem },
    Io {
        context: String,
        source: std::io::Error,
    },
}

impl std::fmt::Display for SubmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SubmitError::NotInterpreted { name } => write!(
                f,
                "`{name}` is declared without a `kind:`, so it is a file this session writes \
                 rather than a document it submits"
            ),
            SubmitError::Accumulated { name } => write!(
                f,
                "`{name}` is a findings artifact: report each finding with \
                 `{tool}` and the engine writes the file",
                tool = ArtifactKind::POST_FINDING_TOOL
            ),
            SubmitError::Refused(report) => write!(f, "{report}"),
            SubmitError::File { path, problem } => {
                write!(
                    f,
                    "{}",
                    ArtifactFailure::file(path.clone(), problem.clone())
                )
            }
            SubmitError::Io { context, source } => write!(f, "cannot {context}: {source}"),
        }
    }
}

/// Validates `document` against the kind `spec` declares and, once it is
/// accepted, writes the canonical file the close will read.
///
/// The verdict is the close's own: the same type, the same rules, the
/// same report. What differs is only that the document never was a
/// file — so a session hears its verdict while it can still act, instead
/// of after it has ended.
pub(crate) fn submit(
    spec: &ArtifactSpec,
    run_dir: &Path,
    document: serde_json::Value,
    max_bytes: Option<u64>,
) -> Result<VerifiedArtifact, SubmitError> {
    let ArtifactSpec::Typed { name, kind } = spec else {
        return Err(SubmitError::NotInterpreted {
            name: spec.name().to_string(),
        });
    };
    let path = Path::new(ARTIFACTS_DIR).join(name).display().to_string();

    let (content, yaml) = match kind {
        ArtifactKind::Tasks => {
            let tasks: TasksFile =
                yunta_core::shape::accept(document, &path).map_err(SubmitError::Refused)?;
            let yaml = render(&tasks, &path)?;
            (ArtifactContent::Tasks(tasks), yaml)
        }
        ArtifactKind::Questions => {
            let file: QuestionsFile =
                yunta_core::shape::accept(document, &path).map_err(SubmitError::Refused)?;
            let yaml = render(&file, &path)?;
            (ArtifactContent::Questions(file.questions), yaml)
        }
        ArtifactKind::Findings => return Err(SubmitError::Accumulated { name: name.clone() }),
    };

    let verified = write_canonical(name, &path, run_dir, yaml, max_bytes)?;
    Ok(VerifiedArtifact {
        content,
        ..verified
    })
}

/// Writes the findings file `node` has earned: every finding it posted
/// that still stands, in the order it first posted them.
///
/// A reviewing session reports each finding as it sees it, so the file
/// is what those reports add up to rather than something the session
/// writes at the end — which is what makes a finding survive a session
/// that dies after reporting it. A node that reported nothing gets a
/// file with an empty list: a review that found nothing is a review.
pub(crate) fn derive_findings(
    spec: &ArtifactSpec,
    run_dir: &Path,
    posted: Vec<Finding>,
    max_bytes: Option<u64>,
) -> Result<VerifiedArtifact, SubmitError> {
    let ArtifactSpec::Typed {
        name,
        kind: ArtifactKind::Findings,
    } = spec
    else {
        return Err(SubmitError::NotInterpreted {
            name: spec.name().to_string(),
        });
    };
    let path = Path::new(ARTIFACTS_DIR).join(name).display().to_string();
    let file = FindingsFile::from_findings(posted.clone());
    let yaml = render(&file, &path)?;
    let verified = write_canonical(name, &path, run_dir, yaml, max_bytes)?;
    Ok(VerifiedArtifact {
        content: ArtifactContent::Findings(posted),
        ..verified
    })
}

fn render<T: yunta_core::shape::Document>(document: &T, path: &str) -> Result<String, SubmitError> {
    yunta_core::shape::render(document).map_err(|source| SubmitError::Io {
        context: format!("render `{path}`"),
        source: std::io::Error::other(source.to_string()),
    })
}

/// Writes `yaml` at `artifacts/<name>` and reports it as the close will
/// read it back.
///
/// Through the run's own `scratch/` and a rename, which is atomic on one
/// filesystem: a reader never meets half a document, and the directory
/// the close audits never holds a temporary file nobody declared.
fn write_canonical(
    name: &str,
    path: &str,
    run_dir: &Path,
    yaml: String,
    max_bytes: Option<u64>,
) -> Result<VerifiedArtifact, SubmitError> {
    let bytes = yaml.into_bytes();
    if let Some(ceiling) = max_bytes {
        if bytes.len() as u64 > ceiling {
            return Err(SubmitError::File {
                path: path.to_string(),
                problem: FileProblem::Oversized {
                    bytes: bytes.len() as u64,
                    ceiling,
                },
            });
        }
    }
    let io = |context: String| move |source| SubmitError::Io { context, source };
    let mut file = tempfile::NamedTempFile::new_in(run_dir.join("scratch"))
        .map_err(io(format!("open a temporary file for `{path}`")))?;
    std::io::Write::write_all(&mut file, &bytes).map_err(io(format!("write `{path}`")))?;
    file.persist(run_dir.join(path))
        .map_err(|e| SubmitError::Io {
            context: format!("place `{path}`"),
            source: e.error,
        })?;
    Ok(VerifiedArtifact {
        name: name.to_string(),
        path: PathBuf::from(path),
        content_hash: sha256_hex(&bytes),
        bytes,
        content: ArtifactContent::Opaque,
    })
}

/// The bytes the run stores for `artifact`.
///
/// An interpreted document is re-rendered from what it parsed as, so
/// what the store holds under a kind is always canonical — a node that
/// writes a tasks document in its own spelling and a session that hands
/// the same document over end up at the same object. An opaque
/// artifact's bytes are its own: the engine assumes no format, so it has
/// nothing to render them from.
pub(crate) fn canonical(artifact: &VerifiedArtifact) -> Result<Vec<u8>, SubmitError> {
    let path = artifact.path.display().to_string();
    Ok(match &artifact.content {
        ArtifactContent::Opaque => artifact.bytes.clone(),
        ArtifactContent::Tasks(tasks) => render(tasks, &path)?.into_bytes(),
        ArtifactContent::Findings(findings) => {
            render(&FindingsFile::from_findings(findings.clone()), &path)?.into_bytes()
        }
        ArtifactContent::Questions(questions) => render(
            &QuestionsFile {
                questions: questions.clone(),
            },
            &path,
        )?
        .into_bytes(),
    })
}
