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
//! [`store`] holds the bytes and writes the view; [`ingest`] judges what
//! a node produced and renders what the engine writes.

mod ingest;
pub mod store;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use yunta_core::events::artifacts::{ArtifactLedger, ArtifactRef};
use yunta_core::events::{ArtifactAcceptedPayload, ArtifactId, ArtifactOrigin, EventPayload};
use yunta_core::{sha256_hex, ArtifactKind, ContentHash, NodeId, ARTIFACTS_DIR};

use crate::run_log::RunLog;
use store::ObjectStore;

pub(crate) use ingest::{canonical, derive_findings, submit, verify_one, SubmitError};
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

/// What a run already holds for the exact bytes `hash` names, read from
/// that run's own log.
///
/// This is where an artifact one run hands to another gets its identity
/// and its producer: the same bytes are the same artifact, whatever file
/// name they were handed over under, and an interpreted document's name
/// is deliberately not on the log to be matched against. `None` when
/// that log accepted no such bytes — a file under its `artifacts/` that
/// no acceptance accounts for, which the receiving run then holds as
/// opaque under the name it mounted it as.
pub(crate) fn handed_over(
    source: &[yunta_core::events::StoredEvent],
    hash: &ContentHash,
) -> Option<ArtifactRef> {
    ArtifactLedger::of(source)
        .every()
        .filter(|held| held.content_hash == *hash)
        .max_by_key(|held| held.seq)
        .cloned()
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
    /// is the node doing its job, and writing one twice is a session
    /// that corrected itself. `producers` is every node the workflow
    /// declares, which is what tells the engine's own view apart from a
    /// session's write.
    pub(crate) fn undeclared_writes(
        &self,
        run_dir: &Path,
        declared: &[String],
        producers: &[&NodeId],
    ) -> std::io::Result<Vec<PathBuf>> {
        let owned: BTreeSet<PathBuf> = declared
            .iter()
            .map(|name| Path::new(ARTIFACTS_DIR).join(name))
            .collect();
        // Two things under `artifacts/` are the engine's own and can
        // land while any session is open, so neither is ever a
        // session's write: the answers it records beside a `questions`
        // artifact, and the view it projects under each producer.
        let engine_written = |path: &Path| {
            path.to_string_lossy().ends_with(ANSWERS_SUFFIX) || store::is_view(path, producers)
        };
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
