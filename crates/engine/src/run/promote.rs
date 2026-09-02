//! Creating a promotion successor: a run that closed
//! `run_finished: promoted` gets a fresh run in `suggested_mode`,
//! `promoted_from` it, inheriting its `artifacts/` wholesale at birth
//! (the successor's initial context automatically includes the
//! predecessor's artifacts, ledger, and findings — all three are files
//! under `artifacts/`, so one directory covers them). Lives in the
//! engine so both drivers of a chain use the identical mechanics: the
//! CLI's `drive_promotions` for top-level runs, and `workflow_exec` for
//! a `kind: workflow` child that promotes mid-composition.

use std::path::{Path, PathBuf};

use yunta_core::{Clock, IdSource, Isolation, Manifest, ModeName, RunId};
use yunta_storage::AsyncStorage;

use super::{create_run, BirthArtifact, CreateRunParams, RunError};

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
    manifest.base_commit = head_commit(predecessor_worktree)?;

    let worktree = match manifest.isolation {
        Isolation::Worktree => {
            let worktree = roots.worktrees.join(successor_id.as_str());
            crate::worktree::prepare_worktree(
                repo,
                &worktree,
                &manifest.base_commit,
                &format!("yunta/{successor_id}"),
                Isolation::Worktree,
            )
            .await?;
            worktree
        }
        Isolation::None => predecessor_worktree.to_path_buf(),
    };

    let inherited =
        read_inherited_artifacts(predecessor_run_dir).map_err(|source| RunError::Io {
            context: format!("inherit artifacts from `{predecessor_id}`"),
            source,
        })?;
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

/// Automatic inheritance: every file directly under the predecessor's
/// `artifacts/` is carried into the successor at birth. Deliberately
/// narrower than the general linked-run mounting a composed workflow
/// run uses.
fn read_inherited_artifacts(from_run_dir: &Path) -> std::io::Result<Vec<BirthArtifact>> {
    let from = from_run_dir.join("artifacts");
    if !from.exists() {
        return Ok(Vec::new());
    }
    let mut inherited = Vec::new();
    for entry in std::fs::read_dir(&from)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().into_string().map_err(|name| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("artifact name `{}` is not UTF-8", name.to_string_lossy()),
            )
        })?;
        inherited.push(BirthArtifact {
            name,
            bytes: std::fs::read(entry.path())?,
        });
    }
    Ok(inherited)
}

fn head_commit(worktree: &Path) -> Result<String, RunError> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(worktree)
        .output()
        .map_err(|e| RunError::Git {
            context: format!("resolve HEAD in `{}`", worktree.display()),
            detail: e.to_string(),
        })?;
    if !output.status.success() {
        return Err(RunError::Git {
            context: format!("resolve HEAD in `{}`", worktree.display()),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
