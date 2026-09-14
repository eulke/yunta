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
pub use ingest::{close_artifacts, ArtifactContent, VerifiedArtifact};
pub(crate) use ingest::{held_document, interpreted, verify_one};
pub use integrity::{ArtifactFault, ArtifactIntegrity};
pub use store::ObjectError;

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

/// Whether the run's own log answers for an artifact a node of
/// `node_kind` declares under `kind`, rather than a file that node wrote
/// in its staging.
///
/// A typed artifact of a session node is never a file that session
/// wrote: `tasks` and `questions` arrive through the submission tool and
/// `findings` are derived from what the node posted. A `kind: workflow`
/// node produces no file at all: everything it declares is taken over
/// from its child run's log. Each is accepted where it is produced, with
/// the origin that produced it — so for those the log is both the only
/// answer and the whole answer. Everything else a node declares is a
/// file it wrote, and the close is where that file enters the run.
///
/// Two decisions turn on this one question — where a close looks for
/// what a node declared, and whether its acceptance is still owed — so
/// it is answered once here.
pub(crate) fn answered_by_the_log(node_kind: &NodeKind, kind: Option<ArtifactKind>) -> bool {
    match node_kind {
        NodeKind::Prompt { .. } | NodeKind::Loop { .. } => kind.is_some(),
        NodeKind::Workflow { .. } => true,
        _ => false,
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
        .map_err(|source| AcceptError::Project { name, source })?;
    Ok(ArtifactRef {
        producer: producer.cloned(),
        artifact,
        content_hash,
        origin: yunta_core::events::ArtifactOrigin::Recorded(origin),
        seq,
    })
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
