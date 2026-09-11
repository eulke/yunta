//! Artifact verification at node close.
//!
//! When a node finishes, everything it declared under
//! `artifacts.produces` must exist and be non-empty under the run's
//! `artifacts/` directory — no matter what the agent reported. Opaque
//! artifacts are verified by existence and content hash only, never by
//! format. `task-ledger`, `findings` and `questions` are the interpreted
//! kinds: each is read through the frontier that names every problem at
//! once, and the parsed result handed back to the caller — `Ledger` for
//! `task_registered`, `Finding`s for `finding_posted`, `Question`s so
//! `node_exec.rs` can pause the run instead of finishing the node.
//!
//! Every failure here is a [`Report`], whatever its cause. A missing
//! file and a ledger with four broken rules are both "this document is
//! not what the node declared", and giving them one shape is what lets
//! the node's failure carry its diagnostics onto the log and render
//! once, for whichever reader is about to see it.

use std::path::{Path, PathBuf};

use yunta_core::diagnostic::{Diagnostic, DocumentKind, DocumentRef, Problem, Report, Subject};
use yunta_core::events::Finding;
use yunta_core::shape::{read, Shaped};
use yunta_core::FindingsFile;
use yunta_core::{
    sha256_hex, ArtifactKind, ArtifactSpec, ContentHash, Ledger, Node, Question, QuestionsFile,
};

/// One declared artifact that passed verification — the data
/// `artifact_written` needs (run-dir-relative path + content hash), plus
/// the parsed ledger or findings when the artifact carries one of those
/// interpreted kinds.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedArtifact {
    pub name: String,
    /// Relative to the run directory (`artifacts/<name>`).
    pub path: PathBuf,
    pub content_hash: ContentHash,
    /// The declared `kind:`, if any — carried onto `artifact_written`
    /// so replay can recognize interpreted artifacts by type.
    pub kind: Option<ArtifactKind>,
    pub ledger: Option<Ledger>,
    pub findings: Option<Vec<Finding>>,
    pub questions: Option<Vec<Question>>,
}

/// One problem with the file itself, before anything inside it is read.
fn about_the_file(document: DocumentRef, code: &'static str, detail: String) -> Report {
    Report::new(
        document,
        vec![Diagnostic::new(
            Subject::Document,
            Problem::rule(code, detail),
        )],
    )
}

/// The shape a node's declared artifact should have, when the engine
/// interprets it at all. The one place a kind maps to its published
/// text, so no door can render a different one.
pub fn published_shape(kind: DocumentKind) -> &'static str {
    match kind {
        DocumentKind::TaskLedger => Ledger::EXAMPLE,
        DocumentKind::Findings => FindingsFile::EXAMPLE,
        DocumentKind::Questions => QuestionsFile::EXAMPLE,
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
) -> Result<Vec<VerifiedArtifact>, Vec<Report>> {
    let Some(artifacts) = &node.artifacts else {
        return Ok(Vec::new());
    };

    let mut verified = Vec::new();
    let mut reports = Vec::new();

    for spec in &artifacts.produces {
        let (name, kind) = match spec {
            ArtifactSpec::Plain(name) => (name, None),
            ArtifactSpec::Typed { name, kind } => (name, Some(kind)),
        };

        let relative = Path::new("artifacts").join(name);
        let full_path = run_dir.join(&relative);
        let document = DocumentRef::new(
            kind.cloned().map(DocumentKind::from),
            relative.display().to_string(),
        );

        let bytes = match std::fs::read(&full_path) {
            Ok(bytes) => bytes,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                reports.push(about_the_file(
                    document,
                    "artifact-missing",
                    format!(
                        "node `{}` declared this artifact and never produced it",
                        node.id
                    ),
                ));
                continue;
            }
            Err(source) => {
                reports.push(about_the_file(
                    document,
                    "artifact-unreadable",
                    format!("exists but cannot be read: {source}"),
                ));
                continue;
            }
        };

        if bytes.is_empty() {
            reports.push(about_the_file(
                document,
                "artifact-empty",
                "is empty; a declared artifact must have content".to_string(),
            ));
            continue;
        }

        // A runaway artifact fails the node with both numbers on the
        // table, never a truncation.
        if let Some(max_bytes) = max_bytes {
            if bytes.len() as u64 > max_bytes {
                reports.push(about_the_file(
                    document,
                    "artifact-oversized",
                    format!(
                        "is {} bytes; `limits.max_artifact_bytes` is {max_bytes}",
                        bytes.len()
                    ),
                ));
                continue;
            }
        }

        let mut ledger = None;
        let mut findings = None;
        let mut questions = None;
        let interpreted = match kind {
            Some(ArtifactKind::TaskLedger) => {
                interpret::<Ledger>(&bytes, document, crate::ledger::register)
                    .map(|parsed| ledger = Some(parsed))
            }
            Some(ArtifactKind::Findings) => {
                interpret::<FindingsFile>(&bytes, document, crate::findings::register).map(
                    |parsed| {
                        findings = Some(parsed.findings.into_iter().map(Finding::from).collect())
                    },
                )
            }
            Some(ArtifactKind::Questions) => {
                interpret::<QuestionsFile>(&bytes, document, crate::questions::register)
                    .map(|parsed| questions = Some(parsed.questions))
            }
            None => Ok(()),
        };
        if let Err(report) = interpreted {
            reports.push(report);
            continue;
        }

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

    if reports.is_empty() {
        Ok(verified)
    } else {
        Err(reports)
    }
}

/// Reads one interpreted artifact: its shape first, then the rules that
/// only hold across the whole document. The two are sequential because
/// no rule can run on a document that did not parse, and reporting
/// shape problems together with rules that could not be evaluated would
/// claim knowledge the engine does not have.
fn interpret<T: Shaped>(
    bytes: &[u8],
    document: DocumentRef,
    rules: fn(&T) -> Vec<Diagnostic>,
) -> Result<T, Report> {
    let parsed = read::<T>(bytes, document.clone())?;
    let broken = rules(&parsed);
    if broken.is_empty() {
        Ok(parsed)
    } else {
        Err(Report::new(document, broken))
    }
}

/// How a node's failure reads to a person: every failing artifact's own
/// block, in declaration order. The one place artifact verification
/// turns into prose.
pub fn render_for_person(reports: &[Report]) -> String {
    reports
        .iter()
        .map(Report::for_person)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every diagnostic behind a node's artifact failure, flattened for the
/// `node_failed` event — the form a later reader can render its own way
/// and a receipt can count without reading prose.
pub fn diagnostics_of(reports: &[Report]) -> Vec<Diagnostic> {
    reports
        .iter()
        .flat_map(|report| report.diagnostics.iter().cloned())
        .collect()
}

/// What the writer of a failed artifact is told, so a repair attempt
/// starts from the problems and the shape rather than from the same
/// prompt that already produced the wrong file. `None` when nothing
/// failed in a way a rewrite could fix.
pub fn render_for_agent(reports: &[Report]) -> Option<String> {
    let instructions: Vec<String> = reports
        .iter()
        .map(|report| {
            let shape = report.document.kind.map(published_shape);
            report.for_agent(shape)
        })
        .collect();
    (!instructions.is_empty()).then(|| instructions.join("\n\n"))
}
