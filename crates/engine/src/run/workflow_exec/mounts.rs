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
//! dangling behind it. A mount of a tasks document is a document the
//! child holds from birth like any other: the child registers its tasks
//! then, `done` the ones the source finished in a tree the child's own
//! descends from, which is `crate::tasks`'s call and not this module's.

use yunta_core::diagnostic::ArtifactFailure;
use yunta_core::events::ArtifactId;
use yunta_core::events::{Failure, MountError, StoredEvent};
use yunta_core::{MountArtifact, MountSpec, NodeKind, RunId};

use super::runs_root;
use crate::artifacts::StoreError;
use crate::replay::RunState;
use crate::run::promote::birth_artifact;
use crate::run::{read_manifest, BirthArtifact, RunCtx};

/// Why the parent's node fails instead of giving birth to the child, in
/// the shapes a node failure has.
#[derive(Debug)]
pub(super) enum NotMounted {
    /// The source run holds nothing under the identity the mount names.
    /// That is a declared artifact that did not close, recorded as one,
    /// so a receipt counts it and `status` attributes it by artifact
    /// like any other.
    Undelivered(ArtifactFailure),
    /// The engine cannot reach what the source run holds under the mount:
    /// the typed reason it cannot.
    Unreachable(MountError),
    /// The source run's store failed to read the bytes it names, an engine
    /// step that failed on its own; `engine_error` is the sentence it states.
    Engine(String),
}

impl From<MountError> for NotMounted {
    fn from(error: MountError) -> Self {
        NotMounted::Unreachable(error)
    }
}

impl From<NotMounted> for Failure {
    /// The single border where each shape becomes what the log records: an
    /// artifact entry, or the typed cause the mount found.
    fn from(problem: NotMounted) -> Self {
        match problem {
            NotMounted::Undelivered(failure) => Failure::artifacts(vec![failure]),
            NotMounted::Unreachable(error) => Failure::MountUnreachable {
                error: Box::new(error),
            },
            NotMounted::Engine(engine_error) => Failure::EngineFailed { engine_error },
        }
    }
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
    state: &RunState,
    events: &[StoredEvent],
    mounts: &[MountSpec],
) -> Result<Vec<BirthArtifact>, NotMounted> {
    let mut resolved = Vec::new();
    for mount in mounts {
        resolved.push(resolve_one(ctx, state, events, mount).await?);
    }
    Ok(resolved)
}

/// The mount's account of bytes its source run cannot hand over: the object
/// holds other bytes than the run accepted, or the store failed to read it.
fn unreadable(source: StoreError, m: &MountArtifact, source_run: &RunId) -> NotMounted {
    match source {
        StoreError::Object(problem) => NotMounted::Unreachable(MountError::Unreadable {
            artifact: m.id.clone(),
            node: m.node.clone(),
            run: source_run.clone(),
            source: problem,
        }),
        StoreError::Io { .. } => NotMounted::Engine(format!(
            "mount of the {} from node `{}`: run `{}` cannot hand over its bytes",
            m.id, m.node, source_run
        )),
    }
}

/// One mount, resolved to the bytes the child is born holding.
async fn resolve_one(
    ctx: &RunCtx<'_>,
    state: &RunState,
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
            let child = state.children.finished_for(&m.node).cloned();
            match child {
                Some(child_id) => child_id,
                None => {
                    return Err(MountError::NoTerminalChild {
                        artifact: m.id.clone(),
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
    let (source_events, producer) = if source_run == *ctx.run_id {
        (events.to_vec(), Some(m.node.clone()))
    } else {
        read_manifest(&crate::run_dir::manifest_path(&source_dir))
            .await
            .map_err(|source| MountError::Unfrozen {
                artifact: m.id.clone(),
                node: m.node.clone(),
                run: source_run.clone(),
                detail: yunta_core::describe(&source),
            })?;
        // The mount names the log and that it cannot be read; the storage
        // failure behind that is kept as its text, for the one who reads why.
        let child_events = ctx
            .storage
            .events_for_run(source_run.clone())
            .await
            .map_err(|source| MountError::Unlogged {
                artifact: m.id.clone(),
                node: m.node.clone(),
                run: source_run.clone(),
                source: source.to_string(),
            })?;
        (child_events, None)
    };
    let held = crate::artifacts::RunArtifacts::of(&source_dir, &source_events);
    // The mount names the identity itself, and a run answers by
    // identity: the same question in both runs, with nothing in between
    // to translate.
    let artifact = ArtifactId::from(&m.id);
    let found = held
        .held(&artifact, producer.as_ref())
        .ok_or_else(|| {
            NotMounted::Undelivered(ArtifactFailure::Unheld {
                run: source_run.clone(),
                producer: Some(m.node.clone()),
                artifact: artifact.clone(),
            })
        })?
        .clone();
    let bytes = held
        .bytes(&found)
        .await
        .map_err(|source| unreadable(source, m, &source_run))?;
    // `as:` gives an opaque artifact another name in the child, and a
    // name is what identifies an opaque artifact — so the child holds
    // the renamed one. A document the engine reads has no name to
    // change, which is why `as:` beside a `kind:` is refused when the
    // workflow is read.
    let carried = match &m.rename {
        Some(name) => ArtifactId::Opaque { name: name.clone() },
        None => artifact,
    };
    Ok(birth_artifact(carried, bytes, &source_run, &found))
}
