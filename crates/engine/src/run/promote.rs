//! Creating a promotion successor: a run that closed
//! `run_finished: promoted` gets a fresh run in `suggested_mode`,
//! `promoted_from` it, inheriting every artifact the predecessor's log
//! holds at birth (the successor's initial context automatically
//! includes the predecessor's artifacts, tasks document and findings —
//! all three are artifacts of that run, so one rule covers them). Lives
//! in the engine so the two places that drive a chain share one mechanic: the
//! CLI's `drive_promotions` for top-level runs, and `workflow_exec` for
//! a `kind: workflow` child that promotes mid-composition.
//!
//! A run never changes its mode or its manifest. When the work outgrows its
//! mode, a person chooses to promote at the escalation that offers it, and
//! the run closes `promoted` while this successor is born, in the next mode
//! of the workflow's declaration order and from the same frozen inputs; the
//! chain is on both logs. There is no way back to an earlier mode.
//!
//! The successor's tree branches from where the predecessor's work left
//! it, so every commit that run integrated is an ancestor of the
//! successor's HEAD and the tasks it finished are finished here too.
//! What a handed-over tasks document means to the run receiving it is
//! `crate::tasks`'s call, made at birth against that tree; this module
//! only says which run handed it over and which tree it lands in.

use std::path::{Path, PathBuf};

use yunta_core::events::artifacts::ArtifactRef;
use yunta_core::events::{ArtifactId, StoredEvent};
use yunta_core::{
    ContentHash, IdSource, Isolation, Manifest, ModeName, Redactor, RunId, SecretSource,
};
use yunta_storage::AsyncStorage;

use crate::artifacts::HeldError;

use super::{
    create_run, run_redactor, BirthArtifact, BirthBaseline, BirthOrigin, CreateRunParams, RunError,
};

/// Everything the successor needs to be executed — the caller drives it
/// through its own `execute_run` (with its own interaction surface,
/// clock and cancellation).
pub struct PromotionSuccessor {
    pub run_id: RunId,
    pub manifest: Manifest,
    pub run_dir: PathBuf,
    pub worktree: PathBuf,
}

/// Where a successor is placed: the runs root its run.dir goes under
/// and the worktrees root its tree goes under.
pub struct RunRoots<'a> {
    pub runs: &'a Path,
    pub worktrees: &'a Path,
}

/// The run [`create_promotion_successor`] builds on top of — the
/// input-side mirror of [`PromotionSuccessor`] itself.
pub struct Predecessor<'a> {
    pub id: &'a RunId,
    pub manifest: &'a Manifest,
    pub worktree: &'a Path,
    pub run_dir: &'a Path,
}

/// What the caller brings to a run it creates: where the log goes, the
/// instant and the ids the log is stamped with, and the supervision every
/// subprocess that creation spawns is born under.
///
/// Grouped because they travel together and none of them is a decision
/// this function makes: it is handed the caller's infrastructure and
/// trail, exactly as [`create_run`] is.
pub struct CallerInfra<'a> {
    pub storage: &'a AsyncStorage,
    pub ids: &'a dyn IdSource,
    /// The token, the clock and the environment every subprocess this
    /// creation spawns is born under — the successor's birth reads the
    /// time by the same clock its git answers to.
    pub supervision: crate::process::Supervision<'a>,
    /// What the successor's commands will run with, recorded at its birth.
    pub environment: Option<yunta_core::events::ExecutionEnvironment>,
    /// Where the values of the secrets the successor's config names come
    /// from. Its redactor is resolved from them before its tree is taken.
    pub secrets: &'a dyn yunta_core::SecretSource,
}

/// Creates (never runs) the successor of `predecessor`, which just
/// closed `Promoted` toward `suggested_mode`.
///
/// The successor works in the predecessor's own tree: under
/// `Isolation::Worktree` on a branch of its own cut where that tree stands,
/// and under `Isolation::None` in the same checkout — the lock (if any) is
/// the caller's and only releases when the whole chain ends. `caller` is
/// the caller's infrastructure and trail, as in [`create_run`].
pub async fn create_promotion_successor(
    predecessor: Predecessor<'_>,
    suggested_mode: &ModeName,
    roots: RunRoots<'_>,
    caller: CallerInfra<'_>,
) -> Result<PromotionSuccessor, RunError> {
    let CallerInfra {
        storage,
        ids,
        supervision,
        environment,
        secrets,
    } = caller;
    let successor_id = ids.mint_run_id(supervision.clock.now());

    let (manifest, redactor) = successor_manifest(&predecessor, secrets, supervision).await?;
    let worktree = successor_tree(&manifest, &predecessor, &successor_id, supervision).await?;
    let inherited = inherit_from_predecessor(storage, &predecessor, &manifest).await?;
    let run_dir = create_run(
        CreateRunParams {
            run_id: &successor_id,
            manifest: &manifest,
            runs_root: roots.runs,
            mode: suggested_mode,
            worktree: &worktree,
            promoted_from: Some(predecessor.id),
            artifacts: &inherited.artifacts,
            baseline: inherited.baseline.as_ref(),
            environment,
            redactor: &redactor,
            blobs: &inherited.blobs,
        },
        storage,
        supervision,
    )
    .await?;

    Ok(PromotionSuccessor {
        run_id: successor_id,
        manifest,
        run_dir,
        worktree,
    })
}

/// The successor's manifest, based where its predecessor's tree stands, and its redactor.
async fn successor_manifest(
    predecessor: &Predecessor<'_>,
    secrets: &dyn SecretSource,
    supervision: crate::process::Supervision<'_>,
) -> Result<(Manifest, Redactor), RunError> {
    let mut manifest = predecessor.manifest.clone();
    // The successor builds on wherever the predecessor's own
    // work left the tree, not on the original base.
    manifest.base_commit = crate::worktree::head_commit(predecessor.worktree, supervision).await?;
    // The links it protects through are those of the tree it builds on.
    manifest.loaded_links =
        crate::manifest::loaded_links(predecessor.worktree, &manifest.base_commit, supervision)
            .await
            .map_err(|error| match error {
                crate::manifest::ManifestError::Links(git) => RunError::from(git),
                other => RunError::Broken {
                    diagnostic: other.to_string(),
                },
            })?;
    // Resolved ahead of the checkout's handover: a short declared secret
    // refuses the successor with nothing taken.
    let redactor = run_redactor(&manifest, secrets)?;
    Ok((manifest, redactor))
}

/// The predecessor's checkout as the successor works in it, handed over under worktree isolation.
async fn successor_tree(
    manifest: &Manifest,
    predecessor: &Predecessor<'_>,
    successor_id: &RunId,
    supervision: crate::process::Supervision<'_>,
) -> Result<PathBuf, RunError> {
    // The successor carries on in the predecessor's own checkout, on a
    // branch of its own cut where that tree stands: nothing it built has to
    // be built again.
    Ok(match manifest.isolation {
        Isolation::Worktree => {
            crate::worktree::hand_over_run_checkout(
                predecessor.worktree,
                successor_id,
                supervision,
            )
            .await?;
            predecessor.worktree.to_path_buf()
        }
        Isolation::None => predecessor.worktree.to_path_buf(),
    })
}

/// What the successor takes from its predecessor at birth.
struct Inheritance {
    artifacts: Vec<BirthArtifact>,
    blobs: Vec<(ContentHash, Vec<u8>)>,
    baseline: Option<BirthBaseline>,
}

/// What the predecessor's log, store and lineage carry into the successor at birth.
async fn inherit_from_predecessor(
    storage: &AsyncStorage,
    predecessor: &Predecessor<'_>,
    manifest: &Manifest,
) -> Result<Inheritance, RunError> {
    // What the predecessor's own log says it held, so each inherited
    // artifact keeps the identity and the producer it had there.
    let view = crate::replay::RunView::of(storage.events_for_run(predecessor.id.clone()).await?);
    let artifacts =
        read_inherited_artifacts(predecessor.run_dir, predecessor.id, &view.events).await?;
    // The successor keeps the predecessor's frozen inputs, so it copies their
    // objects from the predecessor's run directory.
    let blobs = match &manifest.frozen {
        Some(frozen) => crate::frozen::store::inherit(predecessor.run_dir, frozen).await?,
        None => Vec::new(),
    };
    // A promotion is the invocation carrying on, so the successor compares
    // against what the lineage measured at its root, never against the tree
    // its predecessor already worked.
    let baseline = crate::run::baseline::inherited(predecessor.id, &view.state);
    Ok(Inheritance {
        artifacts,
        blobs,
        baseline,
    })
}

/// Automatic inheritance: every artifact the predecessor's log holds is
/// carried into the successor at birth. Deliberately narrower than the
/// general linked-run mounting a composed workflow run uses.
///
/// What the predecessor holds is what its log accepted — the standing
/// acceptance of each identity, with the bytes out of its object store —
/// so a file lying under its `artifacts/` that no acceptance accounts
/// for is not an artifact and reaches no successor. Each inherited
/// artifact keeps the identity and the producer the predecessor held it
/// under, because the identity is all a run needs to answer for it.
async fn read_inherited_artifacts(
    from_run_dir: &Path,
    from_run: &RunId,
    from_events: &[StoredEvent],
) -> Result<Vec<BirthArtifact>, HeldError> {
    let held = crate::artifacts::RunArtifacts::of(from_run_dir, from_events);
    let mut inherited = Vec::new();
    for artifact in held.ledger().every() {
        let bytes = held.bytes(artifact).await.map_err(HeldError::from)?;
        inherited.push(birth_artifact(
            artifact.artifact.clone(),
            bytes,
            from_run,
            artifact,
        ));
    }
    Ok(inherited)
}

/// One artifact as the run receiving it holds it: the identity it
/// carries there, and the run and producer the handing-over log states
/// for it.
pub(super) fn birth_artifact(
    artifact: ArtifactId,
    bytes: Vec<u8>,
    from_run: &RunId,
    held: &ArtifactRef,
) -> BirthArtifact {
    BirthArtifact {
        artifact,
        origin: BirthOrigin::Inherited {
            run: from_run.clone(),
            producer: held.producer.clone(),
        },
        bytes,
    }
}
