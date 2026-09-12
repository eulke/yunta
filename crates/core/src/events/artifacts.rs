//! The one fold from artifact events to the artifacts a run holds.
//!
//! An artifact is a fact of the log: `artifact_accepted` states what the
//! artifact is, the hash of its bytes, and how the run came by it. What
//! the run holds now is the last acceptance of each identity — the
//! `artifacts/` directory is a view of that, never the answer to it, so
//! a file somebody replaced cannot make a reader disagree with the log.
//! Every surface that resolves, renders, inherits or counts artifacts
//! reads them from here rather than folding the events itself.
//!
//! Ownership is by node: the identity a node accepts is its own, and two
//! nodes may each hold a tasks document. What the run acquires without a
//! node of its own — a `document` input, a mount, a promotion — carries
//! no producer and stands like any other.
//!
//! A log written before `artifact_accepted` existed folds too: an
//! `artifact_written` states the same identity and hash, with
//! [`ArtifactOrigin::Legacy`] for the origin it never recorded.

use std::collections::BTreeMap;
use std::path::Path;

use super::{ArtifactId, ArtifactOrigin, ArtifactWrittenPayload, EventPayload, StoredEvent};
use crate::hash::ContentHash;
use crate::ids::{NodeId, Seq};
use crate::workflow::{ArtifactKind, ARTIFACTS_DIR};

/// What identifies one artifact within a run: what it is, and which node
/// produced it — `None` for what the run acquired without one.
type Held = (Option<NodeId>, ArtifactId);

/// An artifact as the run holds it now, with the event that put it
/// there.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactRef {
    pub producer: Option<NodeId>,
    pub artifact: ArtifactId,
    pub content_hash: ContentHash,
    pub origin: ArtifactOrigin,
    /// The position of the acceptance this ref reads, which is how two
    /// producers of one identity are ordered against each other.
    pub seq: Seq,
}

/// Every artifact a log has accepted, by the node that produced it.
///
/// Folds in log order and answers two questions: which artifact answers
/// one identity — what a context source, a mount and a submission
/// resolve against — and every artifact the run holds.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ArtifactLedger {
    current: BTreeMap<Held, ArtifactRef>,
    /// The order identities were first accepted in, which is the order
    /// every listing reads in: re-accepting an artifact replaces its
    /// content and moves nothing, so a reader sees the run's artifacts
    /// where they first appeared.
    first_accepted: Vec<Held>,
}

impl ArtifactLedger {
    /// Folds every event of `events`, in order.
    pub fn of<'a>(events: impl IntoIterator<Item = &'a StoredEvent>) -> Self {
        let mut ledger = ArtifactLedger::default();
        for event in events {
            if let Some(payload) = event.payload() {
                ledger.apply(event.node_id.as_ref(), event.seq, payload);
            }
        }
        ledger
    }

    /// Applies one event, `node` being the envelope's producer and `seq`
    /// its position, and answers with the artifact it now holds — `None`
    /// for an event that states no artifact.
    ///
    /// Total, and ignores an event about anything else: the fold reads
    /// the two kinds that state an artifact and nothing more, so a log
    /// from anywhere leaves it with the artifacts it can account for
    /// rather than a panic.
    pub fn apply(
        &mut self,
        node: Option<&NodeId>,
        seq: Seq,
        payload: &EventPayload,
    ) -> Option<&ArtifactRef> {
        let (artifact, content_hash, origin) = match payload {
            EventPayload::ArtifactAccepted(accepted) => (
                accepted.artifact.clone(),
                accepted.content_hash.clone(),
                accepted.origin.clone(),
            ),
            EventPayload::ArtifactWritten(written) => (
                legacy_identity(written),
                written.content_hash.clone(),
                ArtifactOrigin::Legacy,
            ),
            _ => return None,
        };
        let held = (node.cloned(), artifact.clone());
        if !self.current.contains_key(&held) {
            self.first_accepted.push(held.clone());
        }
        self.current.insert(
            held.clone(),
            ArtifactRef {
                producer: node.cloned(),
                artifact,
                content_hash,
                origin,
                seq,
            },
        );
        self.current.get(&held)
    }

    /// The artifact answering `id`, or `None` when the run holds none.
    ///
    /// A `producer` names whose artifact is meant and reaches that
    /// node's alone. Without one the question is about the run: the last
    /// acceptance of that identity by `seq`, whoever produced it.
    pub fn latest(&self, id: &ArtifactId, producer: Option<&NodeId>) -> Option<&ArtifactRef> {
        match producer {
            Some(node) => self.current.get(&(Some(node.clone()), id.clone())),
            None => self
                .current
                .values()
                .filter(|held| &held.artifact == id)
                .max_by_key(|held| held.seq),
        }
    }

    /// Every artifact of one interpreted kind, in first-acceptance
    /// order — one per producer that accepted it.
    pub fn of_kind(&self, kind: ArtifactKind) -> impl Iterator<Item = &ArtifactRef> {
        self.every().filter(
            move |held| matches!(&held.artifact, ArtifactId::Interpreted { kind: k } if *k == kind),
        )
    }

    /// Every artifact one node produced, in first-acceptance order.
    pub fn by_producer(&self, node: &NodeId) -> impl Iterator<Item = &ArtifactRef> {
        let node = node.clone();
        self.every()
            .filter(move |held| held.producer.as_ref() == Some(&node))
    }

    /// Every artifact the run holds, in first-acceptance order.
    pub fn every(&self) -> impl Iterator<Item = &ArtifactRef> {
        self.first_accepted
            .iter()
            .filter_map(|held| self.current.get(held))
    }
}

/// The identity an `artifact_written` states. A declared `artifact_kind`
/// is the identity; without one the artifact is opaque and its name is
/// what the run wrote it as.
fn legacy_identity(written: &ArtifactWrittenPayload) -> ArtifactId {
    ArtifactId::of(&artifact_name(&written.path), written.artifact_kind)
}

/// The name under `artifacts/` of a run-dir-relative path, which is the
/// name a workflow declared — a nested one included. Rebuilt from the
/// path's components rather than sliced out of its text, so the name
/// reads with `/` the way a workflow writes it whatever separator the
/// path carries.
fn artifact_name(path: &Path) -> String {
    path.strip_prefix(ARTIFACTS_DIR)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
