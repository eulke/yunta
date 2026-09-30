//! Every artifact a run holds: the bytes, the fact on the log, and the
//! view on disk.
//!
//! [`accept`] is the one door. Whatever brought an artifact in — a
//! session's submission, a file a command node wrote, a document the
//! engine derived, the answers to a question, another run handing one
//! over — it enters here: the bytes go to the run's object store, an
//! `artifact_accepted` states what the artifact is and how the run came
//! by it, and the `artifacts/` view is written from what was stored. A
//! run's artifacts are therefore exactly what its log says, and the
//! directory is a projection of that rather than the answer to it.
//!
//! [`RunArtifacts`] is the one door out. Every reader — a context
//! source, a loop looking for its tasks, a mount, a promotion, a
//! distillation, a gate's attachment — resolves an artifact against the
//! log and takes its bytes from the store, so nothing in the engine
//! opens an artifact by file name.
//!
//! [`store`] holds the bytes and writes the view; [`ingest`] judges what
//! a node produced; [`canonical`] turns a document into the bytes the
//! run stores for it, whoever wrote it; [`integrity`] asks the store
//! whether it still answers for the log.

mod canonical;
mod ingest;
mod integrity;
pub mod shown;
pub mod store;

use std::path::Path;

use yunta_core::events::artifacts::{ArtifactLedger, ArtifactRef};
use yunta_core::events::{
    ArtifactAcceptedPayload, ArtifactId, EventPayload, RecordedOrigin, StoredEvent,
};
use yunta_core::{ArtifactKind, NodeId, NodeKind, ARTIFACTS_DIR};

use crate::run_log::RunLog;
use store::ObjectStore;
use yunta_core::events::ArtifactEvent;

pub(crate) use canonical::{canonical, canonical_document, derive_findings, submit, SubmitError};
pub use ingest::{close_artifacts, ArtifactContent, StagedHash, VerifiedArtifact};
pub(crate) use ingest::{held_document, interpreted, verify_one};
pub use integrity::{ArtifactFault, ArtifactIntegrity};
pub use store::ObjectError;

/// Why a document the run holds could not be read back: its bytes, or
/// what they say.
#[derive(Debug, thiserror::Error)]
pub(crate) enum HeldError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("{0}")]
    Unreadable(#[from] yunta_core::diagnostic::Report),
}

impl From<HeldError> for crate::run::RunError {
    fn from(error: HeldError) -> Self {
        match error {
            HeldError::Object(source) => Self::Object(source),
            HeldError::Unreadable(report) => Self::UnreadableArtifact(report),
        }
    }
}

/// A document the run holds, read into its type, and how a reader
/// names it: where its view sits, which is the file a person opens.
pub(crate) struct Held<T> {
    pub document: T,
    pub describe: String,
}

/// The document of `T`'s kind the run accepted last, whoever produced
/// it, read out of the object store through the same door a close reads
/// it through — so one that stopped being readable is reported as the
/// document it is, with every problem named.
///
/// The log is the answer, so a reader long after its producer finds the
/// document whatever became of the `artifacts/` view. `None` when the
/// run holds none: no node produced one, no input named one, and nothing
/// was handed over.
pub(crate) async fn latest<T: yunta_core::shape::Document>(
    run_dir: &Path,
    events: &[StoredEvent],
) -> Result<Option<Held<T>>, HeldError> {
    let held = RunArtifacts::of(run_dir, events);
    let Some(last) = held.ledger().of_kind(T::KIND).last().cloned() else {
        return Ok(None);
    };
    let bytes = held.bytes(&last).await?;
    let describe = describe(&last);
    let document = yunta_core::shape::read::<T>(&bytes, describe.clone())?;
    Ok(Some(Held { document, describe }))
}

/// Why an artifact the run acquired did not become a fact of the run.
#[derive(Debug, thiserror::Error)]
pub enum AcceptError {
    #[error("failed to store the bytes of `{name}`")]
    Store {
        name: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write the `{ARTIFACTS_DIR}/` view of `{name}`")]
    Project {
        name: String,
        #[source]
        source: ObjectError,
    },
    #[error("failed to record `{name}` on the run's log")]
    Log {
        name: String,
        #[source]
        source: yunta_storage::StorageError,
    },
}

/// Who answers for one artifact a node declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Answerer {
    /// The run's own log: the artifact entered where it was produced,
    /// with the origin that produced it, so the acceptance standing now
    /// is both the only answer and the whole answer.
    Log,
    /// A file the node wrote in its own staging, which the close reads.
    Staging,
}

/// Who answers for an artifact a node of `node_kind` declares under
/// `kind`.
///
/// A typed artifact of a session node is never a file that session
/// wrote: `tasks` and `questions` arrive through the submission tool and
/// `findings` are derived from what the node posted. A `kind: workflow`
/// node produces no file at all: everything it declares is taken over
/// from its child run's log. The answers to a questions document are the
/// engine's own whatever node asked, because a person replied and the
/// engine wrote them. Everything else a node declares is a file it
/// wrote, and the close is where that file enters the run.
///
/// Three decisions turn on this one question — where a close looks for
/// what a node declared, whether its acceptance is still owed, and what
/// a session reads back when it asks — so it is answered once here.
pub(crate) fn answerer(node_kind: &NodeKind, kind: Option<ArtifactKind>) -> Answerer {
    match (node_kind, kind) {
        (_, Some(ArtifactKind::Answers)) => Answerer::Log,
        (NodeKind::Prompt { .. } | NodeKind::Loop { .. }, Some(_)) => Answerer::Log,
        (NodeKind::Workflow { .. }, _) => Answerer::Log,
        _ => Answerer::Staging,
    }
}

/// Takes one artifact into the run: stores its bytes, records the
/// acceptance, and writes the view.
///
/// In that order, and it is the order that matters. The bytes exist
/// before anything names them, so the hash the event carries always has
/// content behind it; the event lands before the view, so a run that
/// dies mid-accept has a log that already names the artifact and a
/// directory a later projection completes. `producer` is the node whose
/// artifact this is — `None` for what the run acquired without a node of
/// its own.
pub(crate) async fn accept(
    log: &RunLog<'_>,
    run_dir: &Path,
    producer: Option<&NodeId>,
    artifact: ArtifactId,
    bytes: &[u8],
    origin: RecordedOrigin,
) -> Result<ArtifactRef, AcceptError> {
    let store = ObjectStore::at(run_dir);
    let name = artifact.view_name();
    let content_hash = store
        .put(bytes)
        .await
        .map_err(|source| AcceptError::Store {
            name: name.clone(),
            source,
        })?;
    let seq = log
        .record(
            producer,
            EventPayload::Artifacts(ArtifactEvent::Accepted(ArtifactAcceptedPayload::new(
                artifact.clone(),
                content_hash.clone(),
                origin.clone(),
            ))),
        )
        .await
        .map_err(|source| AcceptError::Log {
            name: name.clone(),
            source,
        })?;
    store
        .project(producer, &name, &content_hash)
        .await
        .map_err(|source| AcceptError::Project {
            name: name.clone(),
            source,
        })?;
    // A plan is also read by a person: its view for them sits beside it.
    if let Some(plan) = plan_view(&artifact, bytes) {
        store
            .write_view(producer, crate::tasks::view::VIEW_NAME, plan.as_bytes())
            .await
            .map_err(|source| AcceptError::Project { name, source })?;
    }
    Ok(ArtifactRef {
        producer: producer.cloned(),
        artifact,
        content_hash,
        origin: yunta_core::events::ArtifactOrigin::Recorded(origin),
        seq,
    })
}

/// The Markdown view of a tasks document, for a person reviewing the
/// plan; `None` for any other artifact.
fn plan_view(artifact: &ArtifactId, bytes: &[u8]) -> Option<String> {
    let ArtifactId::Interpreted {
        kind: yunta_core::ArtifactKind::Tasks,
    } = artifact
    else {
        return None;
    };
    let file =
        yunta_core::shape::read::<yunta_core::TasksFile>(bytes, artifact.view_name()).ok()?;
    Some(crate::tasks::view::PlanView::of(&file).markdown())
}

/// What one run holds, as its own log states it: the artifacts it has
/// accepted, and the bytes behind each.
///
/// The one way anything reads an artifact. Resolution is by the log — the
/// acceptance standing now for an identity — and the bytes come from the
/// object store by hash, so no reader can disagree with the run's own
/// history and a file somebody replaced under `artifacts/` changes
/// nothing. That directory is the view; this is the answer.
pub(crate) struct RunArtifacts<'a> {
    ledger: ArtifactLedger,
    store: ObjectStore<'a>,
}

impl<'a> RunArtifacts<'a> {
    /// What the run rooted at `run_dir` holds, folded from `events` —
    /// that run's own log.
    pub(crate) fn of(run_dir: &'a Path, events: &[StoredEvent]) -> Self {
        RunArtifacts {
            ledger: ArtifactLedger::of(events),
            store: ObjectStore::at(run_dir),
        }
    }

    /// Every artifact the run holds, to ask by kind, by producer or in
    /// full.
    pub(crate) fn ledger(&self) -> &ArtifactLedger {
        &self.ledger
    }

    /// The artifact `id` names, as `producer` holds it — without a
    /// producer, the last acceptance of that identity by anyone. `None`
    /// when the run holds none.
    pub(crate) fn held(&self, id: &ArtifactId, producer: Option<&NodeId>) -> Option<&ArtifactRef> {
        self.ledger.latest(id, producer)
    }

    /// The bytes `held` names, verified against its hash.
    pub(crate) async fn bytes(&self, held: &ArtifactRef) -> Result<Vec<u8>, ObjectError> {
        self.store.get(&held.content_hash).await
    }
}

/// How a diagnostic names one artifact the run holds: where its view
/// sits, which is the file a reader opens.
pub(crate) fn describe(held: &ArtifactRef) -> String {
    store::view_path(held.producer.as_ref(), &held.artifact.view_name())
        .display()
        .to_string()
}
