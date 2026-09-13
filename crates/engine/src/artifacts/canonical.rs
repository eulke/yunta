//! Turning a document into the bytes the run stores for it.
//!
//! A document reaches the run three ways — a session hands it over, the
//! engine derives it from what a node posted, or it arrives as a file
//! somebody else wrote — and all three end here, because what the run
//! holds under a kind is never the text it was given: it is the document
//! re-rendered from what it parsed as. Two sessions that mean the same
//! thing therefore produce the same object, and a replay reproduces it.
//!
//! The rendering is also the last place a document can be refused: it is
//! read through its kind's own door, held to `limits.max_artifact_bytes`
//! like any file, and rejected with every problem it has. Nothing here
//! writes: bytes reach the run through [`accept`](super::accept), which
//! is what stores them, states them on the log and writes the view.

use std::path::PathBuf;

use yunta_core::diagnostic::{
    ArtifactFailure, Diagnostic, DocumentRef, FileProblem, Problem, Report, Subject,
};
use yunta_core::events::{ArtifactId, Finding};
use yunta_core::{
    sha256_hex, ArtifactKind, ArtifactSpec, FindingsFile, NodeId, QuestionsFile, TasksFile,
};

use super::ingest::{interpret, ArtifactContent, VerifiedArtifact};

/// Why a document a session handed over did not become a file.
#[derive(Debug)]
pub(crate) enum SubmitError {
    /// The declaration is an opaque artifact — a file the session writes
    /// itself, with no document for the engine to validate.
    NotInterpreted { name: String },
    /// The declaration is a findings artifact, whose entries arrive one
    /// at a time and whose file the engine derives at close. Defensive:
    /// no submission tool is offered for the kind.
    Accumulated,
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
            SubmitError::Accumulated => write!(
                f,
                "a findings artifact is not submitted whole: report each finding with \
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

/// Validates `document` against the kind `spec` declares and hands back
/// the canonical rendering the run accepts.
///
/// The verdict is the close's own: the same type, the same rules, the
/// same report. What differs is only that the document never was a
/// file — so a session hears its verdict while it can still act, instead
/// of after it has ended. Nothing is written here: an accepted document
/// goes to the object store through [`accept`](super::accept), which is
/// also what writes its view, and the close asks the log for it.
pub(crate) fn submit(
    node: &NodeId,
    spec: &ArtifactSpec,
    document: serde_json::Value,
    max_bytes: Option<u64>,
) -> Result<VerifiedArtifact, SubmitError> {
    let ArtifactSpec::Interpreted(kind) = spec else {
        return Err(SubmitError::NotInterpreted {
            name: spec.to_string(),
        });
    };
    let artifact = ArtifactId::Interpreted { kind: *kind };
    let path = document_path(node, &artifact);

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
        ArtifactKind::Findings => return Err(SubmitError::Accumulated),
    };

    rendered_document(artifact, path, yaml, content, max_bytes)
}

/// Renders the findings document `node` has earned: every finding it
/// posted that still stands, in the order it first posted them.
///
/// A reviewing session reports each finding as it sees it, so the
/// document is what those reports add up to rather than something the
/// session writes at the end — which is what makes a finding survive a
/// session that dies after reporting it. A node that reported nothing
/// gets an empty list: a review that found nothing is a review.
pub(crate) fn derive_findings(
    node: &NodeId,
    posted: Vec<Finding>,
    max_bytes: Option<u64>,
) -> Result<VerifiedArtifact, SubmitError> {
    let artifact = ArtifactId::Interpreted {
        kind: ArtifactKind::Findings,
    };
    let path = document_path(node, &artifact);
    let file = FindingsFile::from_findings(posted.clone());
    let yaml = render(&file, &path)?;
    rendered_document(
        artifact,
        path,
        yaml,
        ArtifactContent::Findings(posted),
        max_bytes,
    )
}

/// How a document the engine renders for `node` names itself: the
/// `artifacts/` view it is projected to once accepted.
///
/// Such a document is never a file on its way in, so there is no staging
/// path to name it by — and the view is where a reader of the run opens
/// it, which is what a refusal and a diagnostic both have to point at.
fn document_path(node: &NodeId, artifact: &ArtifactId) -> String {
    super::ingest::view_path(node, artifact)
}

fn render<T: yunta_core::shape::Document>(document: &T, path: &str) -> Result<String, SubmitError> {
    yunta_core::shape::render(document).map_err(|source| SubmitError::Io {
        context: format!("render `{path}`"),
        source: std::io::Error::other(source.to_string()),
    })
}

/// The document the run is about to accept: `yaml` as the bytes it will
/// store, under the one guard a rendering can still break.
///
/// `limits.max_artifact_bytes` bounds what the run holds, whoever
/// produced it, so a canonical rendering past the ceiling is refused
/// with both numbers on the table exactly as an oversized file is — the
/// run never accepts an artifact it would have turned away as a file.
fn rendered_document(
    artifact: ArtifactId,
    path: String,
    yaml: String,
    content: ArtifactContent,
    max_bytes: Option<u64>,
) -> Result<VerifiedArtifact, SubmitError> {
    let bytes = yaml.into_bytes();
    if let Some(ceiling) = max_bytes {
        if bytes.len() as u64 > ceiling {
            return Err(SubmitError::File {
                path,
                problem: FileProblem::Oversized {
                    bytes: bytes.len() as u64,
                    ceiling,
                },
            });
        }
    }
    Ok(VerifiedArtifact {
        artifact,
        path: PathBuf::from(path),
        content_hash: sha256_hex(&bytes),
        bytes,
        content,
    })
}

/// The canonical bytes the run would hold for a document of `kind` that
/// arrived as a file of its own, or every problem that document has.
///
/// The two steps every document takes wherever it enters — read through
/// the kind's own door, so its rules run, and rendered back, so what the
/// run holds is the canonical text of what it read — for bytes nobody in
/// the run produced. `path` is where a reader opens that file, so a
/// refusal names the file somebody has to fix rather than the view the
/// run would have written from it.
pub(crate) fn canonical_document(
    kind: ArtifactKind,
    bytes: &[u8],
    path: &str,
) -> Result<Vec<u8>, Report> {
    let content = interpret(Some(kind), bytes, path)?;
    let document = VerifiedArtifact {
        artifact: ArtifactId::Interpreted { kind },
        path: PathBuf::from(path),
        content_hash: sha256_hex(bytes),
        bytes: bytes.to_vec(),
        content,
    };
    canonical(&document).map_err(|error| {
        // A document that parsed and cannot be rendered back is a
        // document the run cannot hold: the reader's answer is the same
        // as for one that never parsed, and it names what failed.
        Report::new(
            DocumentRef::new(kind, path),
            vec![Diagnostic::new(
                Subject::Document,
                Problem::parse("", error.to_string()),
            )],
        )
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
