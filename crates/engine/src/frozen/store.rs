//! Where a run keeps the bytes its frozen set names, and the copies a
//! session mounts or runs.
//!
//! Every frozen input is an object of the run's store, named by its
//! content hash, so two inputs with the same bytes are one object. Skills
//! and executors are also copied under `frozen/` in the run directory,
//! read-only, which is the path a session is given. A copy is never the
//! source of truth: the object is, and a copy is checked against its frozen
//! file set and each file's hash before a session reads it.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use yunta_core::durable::Durability;
use yunta_core::{ContentHash, ExecutorName, Frozen, SkillName};

use super::{executor_input, named_inputs, skill_input, FrozenError};
use crate::artifacts::store::{ObjectError, ObjectStore};

/// The folder under a run directory that holds the copies of skills and
/// executors.
const FROZEN_DIR: &str = "frozen";

/// The mode of a copied skill file, `0o444`: read by everyone and written
/// by no one, so a session cannot change what it mounts.
const SKILL_MODE: u32 = 0o444;

/// The mode of a copied executor, `0o555`: read and executed by everyone
/// and written by no one, so the run executes the bytes it was born with.
const EXECUTOR_MODE: u32 = 0o555;

/// Stores the blobs of a run's frozen set in the run's object store, and
/// copies its skills and executors under the run directory.
///
/// Every hash the frozen set names must be among the blobs or already
/// stored, or the run is not born. A run with no frozen set stores what it
/// was handed and copies nothing.
pub(crate) async fn write_at_birth(
    run_dir: &Path,
    blobs: &[(ContentHash, Vec<u8>)],
    frozen: Option<&Frozen>,
) -> Result<(), FrozenError> {
    let store = ObjectStore::at(run_dir);
    for (_, bytes) in blobs {
        // Whole, not synced: a node verifies a frozen input against its hash
        // each time it reads one, so an input a crash lost fails that node as
        // one that changed, and is never read wrong. A sync for each object
        // would make a run's birth wait on a full disk flush per frozen file.
        let stored = store.put_with(bytes, Durability::Atomic).await;
        stored.map_err(|source| FrozenError::Io {
            context: format!("store a frozen input under `{}`", run_dir.display()),
            source,
        })?;
    }
    let Some(frozen) = frozen else {
        return Ok(());
    };
    for (input, hash) in named_inputs(frozen) {
        if !stored(&store, hash).await? {
            return Err(FrozenError::Missing { input });
        }
    }
    copy_skills(run_dir, frozen).await?;
    copy_executors(run_dir, frozen).await
}

/// The objects a frozen set names, read from another run's store: what a
/// child or a successor run is born holding.
pub(crate) async fn inherit(
    from_run_dir: &Path,
    frozen: &Frozen,
) -> Result<Vec<(ContentHash, Vec<u8>)>, FrozenError> {
    let mut blobs = BTreeMap::new();
    for (input, hash) in named_inputs(frozen) {
        if !blobs.contains_key(hash) {
            let bytes = object(from_run_dir, hash, &input).await?;
            blobs.insert(hash.clone(), bytes);
        }
    }
    Ok(blobs.into_iter().collect())
}

/// Where a run's copy of a skill's directory is.
pub(crate) fn skill_copy(run_dir: &Path, name: &SkillName) -> PathBuf {
    run_dir.join(FROZEN_DIR).join("skills").join(name.as_str())
}

/// Where a run's copy of an executor's binary is.
pub(crate) fn executor_copy(run_dir: &Path, name: &ExecutorName) -> PathBuf {
    run_dir
        .join(FROZEN_DIR)
        .join("executors")
        .join(name.as_str())
}

/// The files a copy holds, relative to `dir` and sorted. An entry that is
/// neither a regular file nor a directory, a symlink included, is a
/// `Mismatch` of `input`: the run never writes one.
pub(crate) async fn copied_files(dir: &Path, input: &str) -> Result<Vec<PathBuf>, FrozenError> {
    let mut files = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let here = dir.join(&relative);
        let io = |source: std::io::Error| FrozenError::Io {
            context: format!("list `{}`", here.display()),
            source,
        };
        let mut listing = tokio::fs::read_dir(&here).await.map_err(io)?;
        while let Some(entry) = listing.next_entry().await.map_err(io)? {
            let path = relative.join(entry.file_name());
            let kind = entry.file_type().await.map_err(io)?;
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                files.push(path);
            } else {
                return Err(FrozenError::Mismatch {
                    input: input.to_string(),
                });
            }
        }
    }
    files.sort();
    Ok(files)
}

/// The bytes of the object `hash` names in the run at `run_dir`, verified
/// against the name. An object the store does not hold is `Missing`; one
/// whose bytes hash to something else is a `Mismatch`.
pub(crate) async fn object(
    run_dir: &Path,
    hash: &ContentHash,
    input: &str,
) -> Result<Vec<u8>, FrozenError> {
    let store = ObjectStore::at(run_dir);
    store.get(hash).await.map_err(|error| match error {
        ObjectError::Missing { .. } => FrozenError::Missing {
            input: input.to_string(),
        },
        ObjectError::Corrupt { .. } => FrozenError::Mismatch {
            input: input.to_string(),
        },
        ObjectError::Io { context, source } => FrozenError::Io { context, source },
    })
}

/// Whether the store holds the object `hash` names.
async fn stored(store: &ObjectStore<'_>, hash: &ContentHash) -> Result<bool, FrozenError> {
    tokio::fs::try_exists(store.path_of(hash))
        .await
        .map_err(|source| FrozenError::Io {
            context: format!("look up object `{hash}`"),
            source,
        })
}

async fn copy_skills(run_dir: &Path, frozen: &Frozen) -> Result<(), FrozenError> {
    for (name, files) in &frozen.skills {
        let dir = skill_copy(run_dir, name);
        create_dir(&dir).await?;
        for (file, hash) in files {
            let bytes = object(run_dir, hash, &skill_input(name, file)).await?;
            write_copy(&dir.join(file), &bytes, SKILL_MODE).await?;
        }
    }
    Ok(())
}

async fn copy_executors(run_dir: &Path, frozen: &Frozen) -> Result<(), FrozenError> {
    for (name, hash) in &frozen.executors {
        let bytes = object(run_dir, hash, &executor_input(name)).await?;
        write_copy(&executor_copy(run_dir, name), &bytes, EXECUTOR_MODE).await?;
    }
    Ok(())
}

async fn create_dir(dir: &Path) -> Result<(), FrozenError> {
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|source| FrozenError::Io {
            context: format!("create `{}`", dir.display()),
            source,
        })
}

/// Writes `bytes` to `path` with `mode`, creating the folders above it.
async fn write_copy(path: &Path, bytes: &[u8], mode: u32) -> Result<(), FrozenError> {
    if let Some(parent) = path.parent() {
        create_dir(parent).await?;
    }
    let io = |context: String| move |source: std::io::Error| FrozenError::Io { context, source };
    tokio::fs::write(path, bytes)
        .await
        .map_err(io(format!("write `{}`", path.display())))?;
    let mut permissions = tokio::fs::metadata(path)
        .await
        .map_err(io(format!("stat `{}`", path.display())))?
        .permissions();
    permissions.set_mode(mode);
    tokio::fs::set_permissions(path, permissions)
        .await
        .map_err(io(format!("set the mode of `{}`", path.display())))
}
