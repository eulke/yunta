//! What a child run is born holding: each `mounts:` entry of a
//! `kind: workflow` node, resolved to bytes before the child exists.
//!
//! The boundary this holds is the one between two runs. A mount names an
//! artifact of *another* run — this parent's own, or a sibling child's —
//! and a run only ever answers about itself: the artifact a name means,
//! who produced it and what its bytes are come from the holding run's own
//! log and object store, never from this one's idea of it. A mount
//! chooses one thing on top of that, the name the child carries the
//! artifact under.
//!
//! Everything resolves in memory, before the child is linked or born, so
//! a source the run cannot hand over fails the parent's node with nothing
//! dangling behind it.

use yunta_core::diagnostic::ArtifactFailure;
use yunta_core::events::{EventPayload, Failure, StoredEvent};
use yunta_core::{MountSpec, NodeKind, RunId};

use super::runs_root;
use crate::run::promote::birth_artifact;
use crate::run::{read_manifest, BirthArtifact, RunCtx};

/// Why the parent's node fails instead of giving birth to the child, in
/// the two shapes a node failure has.
#[derive(Debug)]
pub(super) enum NotMounted {
    /// The source run holds nothing under the identity the mount names.
    /// That is a declared artifact that did not close, recorded as one,
    /// so a receipt counts it and `status` attributes it by artifact
    /// like any other.
    Undelivered(ArtifactFailure),
    /// The engine cannot reach the source at all. No artifact entry
    /// describes that, so it is the one sentence the engine states.
    Unreachable(MountError),
}

impl From<MountError> for NotMounted {
    fn from(error: MountError) -> Self {
        NotMounted::Unreachable(error)
    }
}

impl From<NotMounted> for Failure {
    /// The single border where either shape becomes what the log
    /// records: an artifact entry, or a sentence.
    fn from(problem: NotMounted) -> Self {
        match problem {
            NotMounted::Undelivered(failure) => Failure::artifacts(vec![failure]),
            NotMounted::Unreachable(error) => Failure::message(error.to_string()),
        }
    }
}

/// A source a mount names and the engine cannot reach.
#[derive(Debug, thiserror::Error)]
pub(super) enum MountError {
    #[error(
        "mount `{name}` from node `{node}`: no linked child run of `{node}` reached a terminal \
         state in this run — did this run's mode exclude it?"
    )]
    NoTerminalChild {
        name: String,
        node: yunta_core::NodeId,
    },
    #[error("mount `{name}` from node `{node}`: run `{run}` cannot hand over its bytes")]
    Unreadable {
        name: String,
        node: yunta_core::NodeId,
        run: RunId,
        #[source]
        source: crate::artifacts::ObjectError,
    },
    #[error(
        "mount `{name}` from node `{node}`: the frozen truth of run `{run}` cannot be read: \
         {detail}"
    )]
    Unfrozen {
        name: String,
        node: yunta_core::NodeId,
        run: RunId,
        detail: String,
    },
    /// Boxed: a storage failure carries far more than any other mount
    /// problem, and every caller moves this error by value.
    #[error("mount `{name}` from node `{node}`: the log of run `{run}` cannot be reached")]
    Unlogged {
        name: String,
        node: yunta_core::NodeId,
        run: RunId,
        #[source]
        source: Box<yunta_storage::StorageError>,
    },
}

/// Resolves every declared mount to bytes, in memory, *before*
/// the child is linked or born — a missing source fails the parent's
/// node with nothing dangling.
///
/// Each mount is answered by the log that holds the artifact and the
/// store that keeps its bytes. A `kind: workflow` source resolves
/// through the recorded link (its last `child_run_finished` on this log)
/// to that child run's own log and store; any other node is this run's,
/// and the mount reaches that node's artifact alone. Returns the child's
/// birth artifacts, or the diagnostic to fail the node with.
pub(super) async fn resolve_mounts(
    ctx: &RunCtx<'_>,
    events: &[StoredEvent],
    mounts: &[MountSpec],
) -> Result<Vec<BirthArtifact>, NotMounted> {
    let mut resolved = Vec::new();
    for mount in mounts {
        resolved.push(resolve_one(ctx, events, mount).await?);
    }
    Ok(resolved)
}

/// One mount, resolved to the bytes the child is born holding.
async fn resolve_one(
    ctx: &RunCtx<'_>,
    events: &[StoredEvent],
    mount: &MountSpec,
) -> Result<BirthArtifact, NotMounted> {
    let m = &mount.artifact;
    let target = ctx
        .manifest
        .workflow
        .nodes
        .iter()
        .find(|candidate| candidate.id == m.node);
    let source_run = match target.map(|candidate| &candidate.kind) {
        Some(NodeKind::Workflow { .. }) => {
            let child = events.iter().rev().find_map(|e| match e.payload() {
                Some(EventPayload::ChildRunFinished(p)) if e.node_id.as_ref() == Some(&m.node) => {
                    Some(p.child_run_id.clone())
                }
                _ => None,
            });
            match child {
                Some(child_id) => child_id,
                None => {
                    return Err(MountError::NoTerminalChild {
                        name: m.name.clone(),
                        node: m.node.clone(),
                    }
                    .into());
                }
            }
        }
        _ => ctx.run_id.clone(),
    };
    // The source run's own log is what says which artifact the
    // mounted name is and who produced it there; a mount only
    // chooses the name the child carries it under. A sibling child
    // run answers about itself, so its workflow is the one that
    // reads the name.
    let source_dir = runs_root(ctx).join(source_run.as_str());
    let (source_events, source_workflow, producer) = if source_run == *ctx.run_id {
        (
            events.to_vec(),
            ctx.manifest.workflow.clone(),
            Some(m.node.clone()),
        )
    } else {
        let manifest = read_manifest(&source_dir.join("manifest.yaml")).map_err(|source| {
            MountError::Unfrozen {
                name: m.name.clone(),
                node: m.node.clone(),
                run: source_run.clone(),
                detail: yunta_core::describe(&source),
            }
        })?;
        let child_events = ctx
            .storage
            .events_for_run(source_run.clone())
            .await
            .map_err(|source| MountError::Unlogged {
                name: m.name.clone(),
                node: m.node.clone(),
                run: source_run.clone(),
                source: Box::new(source),
            })?;
        (child_events, manifest.workflow, None)
    };
    let held = crate::artifacts::RunArtifacts::of(&source_dir, &source_events);
    // The source run answers by identity, so the failure records the
    // identity it was asked for rather than the name the mount spells
    // it with here.
    let (artifact, found) = held.identified(&source_workflow, producer.as_ref(), &m.name);
    let found = found
        .ok_or_else(|| {
            NotMounted::Undelivered(ArtifactFailure::Unheld {
                run: source_run.clone(),
                producer: Some(m.node.clone()),
                artifact,
            })
        })?
        .clone();
    let bytes = held
        .bytes(&found)
        .map_err(|source| MountError::Unreadable {
            name: m.name.clone(),
            node: m.node.clone(),
            run: source_run.clone(),
            source,
        })?;
    Ok(birth_artifact(
        m.rename.clone().unwrap_or_else(|| m.name.clone()),
        bytes,
        &source_run,
        &found,
    ))
}
