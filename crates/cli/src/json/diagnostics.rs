//! The artifacts a failed node names, as a run's document carries them:
//! what is wrong with each one, and where a reader goes to look for it.

use std::collections::BTreeMap;

use yunta_core::events::{ArtifactId, EventPayload, Failure, NodeEvent, StoredEvent};
use yunta_core::{ArtifactFailure, Diagnostic, FileProblem, NodeId, Problem, RuleWords};

/// One problem of a document as a program reads it: the diagnostic as the
/// log holds it, and the clause of a broken rule, beside it.
#[derive(serde::Serialize)]
struct DiagnosticJson {
    #[serde(flatten)]
    diagnostic: Diagnostic,
    /// The clause of a rule the log keeps as a fault. Absent on an older
    /// log's rule, which carries its `detail` already, and on a parse
    /// problem, whose `message` is the reader's own report.
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

impl From<&Diagnostic> for DiagnosticJson {
    fn from(diagnostic: &Diagnostic) -> Self {
        let detail = match &diagnostic.problem {
            Problem::Rule {
                words: RuleWords::Fault { fault },
                ..
            } => Some(fault.clause()),
            Problem::Rule {
                words: RuleWords::Logged { .. },
                ..
            }
            | Problem::Parse { .. } => None,
        };
        DiagnosticJson {
            diagnostic: diagnostic.clone(),
            detail,
        }
    }
}

/// One artifact a node's failure names, with what is wrong with it. A
/// node that failed on two artifacts produces two of these, so a reader
/// attributes a problem to the artifact it came from instead of
/// splitting a sentence.
///
/// Every field is absent when the failure has nothing to put there, so
/// no consumer ever meets an invented path or an empty code: an artifact
/// another run owes and a document its node never handed over have no
/// file to open and no content that was read, and a file that was never
/// written has no kind its content could have met.
#[derive(Default, serde::Serialize)]
pub(crate) struct ArtifactProblems {
    /// The stable name of what is wrong with the artifact itself —
    /// `artifact-missing`, `artifact-empty`, `artifact-oversized`,
    /// `artifact-unreadable`, `artifact-undelivered`, `artifact-unheld`.
    /// Absent when the file is there and its content is what failed,
    /// because that is not one problem: each of the document's own
    /// carries its code.
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<yunta_core::diagnostic::DiagnosticCode>,
    /// As a reader would type it to open the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    /// The kind whose shape the content had to meet.
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<yunta_core::diagnostic::DocumentKind>,
    /// What is wrong with the file itself — never written, empty, past
    /// the ceiling, refused by the filesystem.
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<FileProblem>,
    /// The run that owes this artifact and holds none of it — where a
    /// reader goes to look. Absent when this run owes it, which is what
    /// a document its own node never handed over is.
    #[serde(skip_serializing_if = "Option::is_none")]
    run: Option<String>,
    /// The node the artifact was asked of: of `run` when one is named,
    /// of this run otherwise. Absent when the question is about a run as
    /// a whole.
    #[serde(skip_serializing_if = "Option::is_none")]
    producer: Option<String>,
    /// The identity that was asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    artifact: Option<ArtifactId>,
    /// Every problem this document's content has, in document order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    diagnostics: Vec<DiagnosticJson>,
}

impl From<&ArtifactFailure> for ArtifactProblems {
    fn from(failure: &ArtifactFailure) -> Self {
        let code = failure.code();
        match failure {
            ArtifactFailure::File { path, problem } => ArtifactProblems {
                code,
                path: Some(path.clone()),
                file: Some(problem.clone()),
                ..ArtifactProblems::default()
            },
            ArtifactFailure::Undelivered { node, artifact } => ArtifactProblems {
                code,
                producer: Some(node.to_string()),
                artifact: Some(artifact.clone()),
                ..ArtifactProblems::default()
            },
            ArtifactFailure::Content(report) => ArtifactProblems {
                path: Some(report.document.path.clone()),
                kind: Some(report.document.kind),
                diagnostics: report
                    .diagnostics
                    .iter()
                    .map(DiagnosticJson::from)
                    .collect(),
                ..ArtifactProblems::default()
            },
            ArtifactFailure::Unheld {
                run,
                producer,
                artifact,
            } => ArtifactProblems {
                code,
                run: Some(run.to_string()),
                producer: producer.as_ref().map(NodeId::to_string),
                artifact: Some(artifact.clone()),
                ..ArtifactProblems::default()
            },
        }
    }
}

/// The artifacts each node's most recent failure names, with what is
/// wrong with each, keyed by node.
///
/// An entry describes the failure a node is in when it is read, never every
/// failure it has had: a node that failed, repaired and failed again is
/// described by its last failure, and one that started again, or whose
/// latest failure is a sentence naming no document, has no entry at
/// all. A reader asking what is wrong at this read is not asking for a history.
pub(super) fn node_diagnostics(events: &[StoredEvent]) -> BTreeMap<String, Vec<ArtifactProblems>> {
    let mut latest = BTreeMap::new();
    for event in events {
        let Some(node_id) = event.node_id.as_ref() else {
            continue;
        };
        match event.payload() {
            Some(EventPayload::Node(NodeEvent::Failed(p))) => match artifact_entries(&p.failure) {
                Some(entries) => {
                    latest.insert(node_id.to_string(), entries);
                }
                None => {
                    latest.remove(node_id.as_str());
                }
            },
            // A node that started again has left its last failure behind.
            Some(EventPayload::Node(NodeEvent::Started(_))) => {
                latest.remove(node_id.as_str());
            }
            _ => {}
        }
    }
    latest
}

/// The artifacts a failure names, or `None` for a failure that names no
/// document: a sentence, a session, a limit, a check, a scope, a config key,
/// a frozen input, or a typed cause. The node's own entry carries each of
/// those whole.
fn artifact_entries(failure: &Failure) -> Option<Vec<ArtifactProblems>> {
    match failure {
        Failure::Artifacts { artifacts } => {
            Some(artifacts.iter().map(ArtifactProblems::from).collect())
        }
        Failure::Message { .. }
        | Failure::Exited { .. }
        | Failure::SessionDied { .. }
        | Failure::SessionRefused { .. }
        | Failure::SessionNotStopped { .. }
        | Failure::Limit { .. }
        | Failure::TasksStopped { .. }
        | Failure::Red { .. }
        | Failure::ChildFailed { .. }
        | Failure::ScopeViolated { .. }
        | Failure::PathsDenied { .. }
        | Failure::ScopeRequested { .. }
        | Failure::ScopeOwed { .. }
        | Failure::Unset { .. }
        | Failure::FrozenInput { .. }
        | Failure::Unchanged { .. }
        | Failure::PlanUnpassable { .. }
        | Failure::TimedOut { .. }
        | Failure::TokenBudget { .. }
        | Failure::Template { .. }
        | Failure::ArtifactUnreadable { .. }
        | Failure::ArtifactName { .. }
        | Failure::MountUnreachable { .. }
        | Failure::GateChose { .. }
        | Failure::TestsAccepted { .. }
        | Failure::ChildRefused { .. }
        | Failure::NestedTooDeep { .. }
        | Failure::GateArtifactMissing { .. }
        | Failure::CapabilityMissing { .. }
        | Failure::EngineFailed { .. }
        | Failure::CommandDenied { .. }
        | Failure::SkillMissing { .. }
        | Failure::LoopCapped { .. }
        | Failure::Confinement { .. }
        | Failure::ChildrenFailed { .. }
        | Failure::Uncertain { .. }
        | Failure::Rejected { .. }
        | Failure::SentBack { .. }
        | Failure::FindingsOpen { .. }
        | Failure::Interrupted { .. }
        | Failure::ChangesRequested { .. }
        | Failure::PullRequestClosed { .. }
        | Failure::LoopStalled { .. }
        | Failure::NoTasksDocument { .. }
        | Failure::RunnerUnresolved { .. }
        | Failure::FrozenRead { .. } => None,
    }
}
