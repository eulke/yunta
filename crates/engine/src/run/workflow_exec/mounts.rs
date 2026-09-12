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

use yunta_core::events::{EventPayload, StoredEvent};
use yunta_core::{MountSpec, NodeKind, RunId};

use super::runs_root;
use crate::run::promote::birth_artifact;
use crate::run::{read_manifest, BirthArtifact, RunCtx};

/// A mount the child cannot be born with.
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
    #[error(
        "mount `{name}` from node `{node}`: run `{run}` holds no artifact `{name}` — the source \
         node never produced it"
    )]
    Unheld {
        name: String,
        node: yunta_core::NodeId,
        run: RunId,
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
) -> Result<Vec<BirthArtifact>, MountError> {
    let mut resolved = Vec::new();
    for mount in mounts {
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
                    Some(EventPayload::ChildRunFinished(p))
                        if e.node_id.as_ref() == Some(&m.node) =>
                    {
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
                        });
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
        let found = held
            .named(&source_workflow, producer.as_ref(), &m.name)
            .ok_or_else(|| MountError::Unheld {
                name: m.name.clone(),
                node: m.node.clone(),
                run: source_run.clone(),
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
        resolved.push(birth_artifact(
            m.rename.clone().unwrap_or_else(|| m.name.clone()),
            bytes,
            &source_run,
            &found,
        ));
    }
    Ok(resolved)
}
