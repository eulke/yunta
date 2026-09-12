//! The run directory's own layout: the names the engine writes under it,
//! in one place, so writing a path and reading it back cannot disagree.
//!
//! Two of the run's directories answer different questions and are
//! deliberately separate. `artifacts/` is the view: the run writes it
//! from what it holds, by projecting each acceptance, and nothing reads
//! it back. `scratch/` is working space, and one node's [`staging`]
//! under it is the only place that node writes the files it declares —
//! its own, so two nodes that declare the same name never meet, and so
//! the view stays the engine's alone.

use std::path::{Path, PathBuf};

use yunta_core::NodeId;

/// The run directory's working space: everything the engine and its
/// sessions need on disk while a run is alive, and nothing a reader of
/// the run resolves anything through.
pub const SCRATCH_DIR: &str = "scratch";

/// Where nodes' staging directories live under [`SCRATCH_DIR`].
const STAGING_DIR: &str = "staging";

/// Where every node's staging sits, one directory per node id.
pub fn staging_root(run_dir: &Path) -> PathBuf {
    run_dir.join(SCRATCH_DIR).join(STAGING_DIR)
}

/// Where `node` writes the files it declares, absolute.
///
/// A node that declares an opaque artifact receives this directory as a
/// writable root, and the close reads what it declared back from here.
/// It is never inside the worktree — the worktree is the work, and its
/// diff is what the scope check reads — and never inside `artifacts/`,
/// which the run writes from what it has already accepted.
pub fn staging(run_dir: &Path, node: &NodeId) -> PathBuf {
    staging_root(run_dir).join(node.as_str())
}

/// Where one declared artifact of `node` is read from, relative to the
/// run directory — the shape a diagnostic names a missing file by.
pub(crate) fn staged_path(node: &NodeId, name: &str) -> PathBuf {
    Path::new(SCRATCH_DIR)
        .join(STAGING_DIR)
        .join(node.as_str())
        .join(name)
}

/// Opens `node`'s staging for one attempt: the directory exists and it
/// is empty.
///
/// Emptying is the point. An attempt that ends without producing what it
/// declared must not be rescued by a file an earlier attempt left, and a
/// run derives what it holds from what each attempt actually did — so
/// every attempt starts with nothing of its own on disk.
pub(crate) async fn open_staging(run_dir: &Path, node: &NodeId) -> std::io::Result<PathBuf> {
    let dir = staging(run_dir, node);
    match tokio::fs::remove_dir_all(&dir).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    tokio::fs::create_dir_all(&dir).await?;
    Ok(dir)
}
