//! Creating a promotion successor: a run that closed
//! `run_finished: promoted` gets a fresh run in `suggested_mode`,
//! `promoted_from` it, inheriting every artifact the predecessor's log
//! holds at birth (the successor's initial context automatically
//! includes the predecessor's artifacts, tasks document and findings —
//! all three are artifacts of that run, so one rule covers them). Lives
//! in the engine so both drivers of a chain use the identical mechanics: the
//! CLI's `drive_promotions` for top-level runs, and `workflow_exec` for
//! a `kind: workflow` child that promotes mid-composition.
//!
//! The successor's tree branches from where the predecessor's work left
//! it, so the tasks that run finished are finished here too: what a
//! handed-over tasks document means to the run receiving it is
//! `crate::tasks`'s call, made at birth, and this module only says which
//! run handed it over.

use std::path::{Path, PathBuf};

use yunta_core::events::artifacts::ArtifactRef;
use yunta_core::events::{ArtifactId, StoredEvent};
use yunta_core::{Clock, IdSource, Isolation, Manifest, ModeName, RunId};
use yunta_storage::AsyncStorage;

use crate::artifacts::ObjectError;

use super::{create_run, BirthArtifact, BirthOrigin, CreateRunParams, RunError};

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

/// Creates (never runs) the successor of `predecessor`, which just
/// closed `Promoted` toward `suggested_mode`.
///
/// `repo` is the checkout a fresh worktree branches from (the original
/// `cwd` for a top-level chain; the parent run's own tree for a child's).
/// Under `Isolation::None` the successor reuses the predecessor's
/// checkout — the lock (if any) is the caller's and only releases when
/// the whole chain ends. `storage`, `clock` and `ids` are the caller's
/// infrastructure and trail, as in [`create_run`].
pub async fn create_promotion_successor(
    predecessor: Predecessor<'_>,
    repo: &Path,
    suggested_mode: &ModeName,
    roots: RunRoots<'_>,
    storage: &AsyncStorage,
    clock: &dyn Clock,
    ids: &dyn IdSource,
) -> Result<PromotionSuccessor, RunError> {
    let Predecessor {
        id: predecessor_id,
        manifest: predecessor_manifest,
        worktree: predecessor_worktree,
        run_dir: predecessor_run_dir,
    } = predecessor;
    let successor_id = ids.mint_run_id(clock.now());

    let mut manifest = predecessor_manifest.clone();
    // The successor builds on wherever the predecessor's own
    // work left the tree, not on the original base.
    manifest.base_commit = crate::worktree::head_commit(predecessor_worktree).await?;

    let worktree = match manifest.isolation {
        Isolation::Worktree => {
            let worktree = roots.worktrees.join(successor_id.as_str());
            crate::worktree::prepare_worktree(
                repo,
                &worktree,
                &manifest.base_commit,
                &crate::worktree::run_branch(&successor_id),
                Isolation::Worktree,
            )
            .await?;
            worktree
        }
        Isolation::None => predecessor_worktree.to_path_buf(),
    };

    // What the predecessor's own log says it held, so each inherited
    // artifact keeps the identity and the producer it had there.
    let predecessor_events = storage.events_for_run(predecessor_id.clone()).await?;
    let inherited =
        read_inherited_artifacts(predecessor_run_dir, predecessor_id, &predecessor_events)?;
    let run_dir = create_run(
        CreateRunParams {
            run_id: &successor_id,
            manifest: &manifest,
            runs_root: roots.runs,
            mode: suggested_mode,
            promoted_from: Some(predecessor_id),
            artifacts: &inherited,
        },
        storage,
        clock,
    )
    .await?;

    Ok(PromotionSuccessor {
        run_id: successor_id,
        manifest,
        run_dir,
        worktree,
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
fn read_inherited_artifacts(
    from_run_dir: &Path,
    from_run: &RunId,
    from_events: &[StoredEvent],
) -> Result<Vec<BirthArtifact>, ObjectError> {
    let held = crate::artifacts::RunArtifacts::of(from_run_dir, from_events);
    let mut inherited = Vec::new();
    for artifact in held.ledger().every() {
        inherited.push(birth_artifact(
            artifact.artifact.clone(),
            held.bytes(artifact)?,
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
