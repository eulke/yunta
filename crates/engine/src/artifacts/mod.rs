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
//! a node produced and renders what the engine writes.

mod ingest;
pub mod store;

use std::path::Path;

use yunta_core::events::artifacts::{declared_name, ArtifactLedger, ArtifactRef};
use yunta_core::events::{
    ArtifactAcceptedPayload, ArtifactId, ArtifactOrigin, EventPayload, StoredEvent,
};
use yunta_core::{ArtifactKind, NodeId, Workflow, ARTIFACTS_DIR};

use crate::run_log::RunLog;
use store::ObjectStore;

pub(crate) use ingest::{canonical, derive_findings, interpreted, submit, verify_one, SubmitError};
pub use ingest::{close_artifacts, ArtifactContent, VerifiedArtifact};
pub use store::ObjectError;

/// What the engine appends to a `questions` artifact's name when it
/// records the answers beside it.
pub(crate) const ANSWERS_SUFFIX: &str = ".answers.yaml";

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

/// What a workflow declares an artifact as: the name its view carries,
/// and the kind the engine reads it under — `None` for an artifact the
/// engine only carries.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Declared<'a> {
    pub(crate) name: &'a str,
    pub(crate) kind: Option<ArtifactKind>,
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
    declared: Declared<'_>,
    bytes: &[u8],
    origin: ArtifactOrigin,
) -> Result<ArtifactRef, AcceptError> {
    let store = ObjectStore::at(run_dir);
    let name = declared.name.to_string();
    let content_hash = store.put(bytes).map_err(|source| AcceptError::Store {
        name: name.clone(),
        source,
    })?;
    let artifact = ArtifactId::of(declared.name, declared.kind);
    let seq = log
        .record(
            producer,
            EventPayload::ArtifactAccepted(ArtifactAcceptedPayload {
                artifact: artifact.clone(),
                content_hash: content_hash.clone(),
                origin: origin.clone(),
            }),
        )
        .await
        .map_err(|source| AcceptError::Log {
            name: name.clone(),
            source,
        })?;
    store
        .project(producer, declared.name, &content_hash)
        .map_err(|source| AcceptError::Project { name, source })?;
    Ok(ArtifactRef {
        producer: producer.cloned(),
        artifact,
        content_hash,
        origin,
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

    /// The artifact `workflow` calls `name`, as `producer` holds it —
    /// without a producer, the last acceptance of that identity by
    /// anyone. `None` when the run holds none.
    pub(crate) fn named(
        &self,
        workflow: &Workflow,
        producer: Option<&NodeId>,
        name: &str,
    ) -> Option<&ArtifactRef> {
        let id = yunta_core::events::artifacts::declared_identity(workflow, producer, name);
        self.ledger.latest(&id, producer)
    }

    /// The bytes `held` names, verified against its hash.
    pub(crate) fn bytes(&self, held: &ArtifactRef) -> Result<Vec<u8>, ObjectError> {
        self.store.get(&held.content_hash)
    }
}

/// How a diagnostic names one artifact the run holds: where its view
/// sits when the workflow names it, and its identity when nothing does.
///
/// A reader that reports a problem with a document says which document,
/// in the terms the workflow author wrote it in.
pub(crate) fn describe(workflow: &Workflow, held: &ArtifactRef) -> String {
    match view_name(workflow, held) {
        Some(name) => store::view_path(held.producer.as_ref(), &name)
            .display()
            .to_string(),
        None => held.artifact.to_string(),
    }
}

/// The name a run's `artifacts/` view carries one held artifact under.
///
/// An opaque artifact is named by its identity. An interpreted one is
/// named by the declaration it answers — its producer's
/// `artifacts.produces` entry of that kind — and the one document a run
/// derives for itself rather than for a node by the name a successor
/// mounts it as. `None` for an interpreted artifact a run holds with no
/// node behind it and no declaration covering it: nothing in the run
/// names it, which a reader handing it on has to know rather than invent
/// a name.
pub(crate) fn view_name(workflow: &Workflow, held: &ArtifactRef) -> Option<String> {
    match (&held.artifact, &held.producer) {
        (ArtifactId::Opaque { name }, _) => Some(name.clone()),
        (id, Some(node)) => declared_name(workflow, node, id),
        (
            ArtifactId::Interpreted {
                kind: ArtifactKind::Findings,
            },
            None,
        ) => Some(crate::findings::INHERITED_FINDINGS.to_string()),
        (ArtifactId::Interpreted { .. }, None) => None,
    }
}
