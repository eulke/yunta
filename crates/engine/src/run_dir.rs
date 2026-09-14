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

/// The manifest a run froze when it was created: what it runs, which
/// runners it resolves against, which limits it is held to.
pub fn manifest_path(run_dir: &Path) -> PathBuf {
    run_dir.join("manifest.yaml")
}

/// The run's progress note, rewritten at every node close — the file a
/// person opens to see where a live run is.
pub fn progress_path(run_dir: &Path) -> PathBuf {
    run_dir.join("progress.md")
}

/// Where a session's own transcript directory goes, under the scratch.
pub fn sessions_root(run_dir: &Path) -> PathBuf {
    run_dir.join(SCRATCH_DIR).join("sessions")
}

/// Where a loop's tasks get a worktree each, so two tasks of one run
/// never share a checkout.
pub fn task_worktrees(run_dir: &Path) -> PathBuf {
    run_dir.join("task-worktrees")
}

/// The run's view of what it holds: one file per artifact, written from
/// the acceptance that named it.
pub fn artifacts_view(run_dir: &Path) -> PathBuf {
    run_dir.join(yunta_core::ARTIFACTS_DIR)
}

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

/// Whose work a node's staging directory holds when the work about to
/// run opens it.
///
/// The directory belongs to the session, not to the attempt. A session
/// picked back up under `on_interrupt: resume_session` goes on writing
/// where it was writing, so what it left there is work it did and
/// emptying would take it away — an artifact it already wrote and does
/// not write again would close the node as undelivered. Everything
/// else opens on nothing: a command node has no session to continue, and
/// a fresh session replacing an interrupted one did not write what the
/// interrupted one left, so a file from an earlier attempt must not
/// close this one as work it never did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Opening {
    /// The session that wrote here is the one about to continue.
    ContinuedSession,
    /// Nothing continues here.
    Fresh,
}

/// Opens `node`'s staging for the work about to run: the directory
/// exists, holding only what [`Opening`] says is still that work's own.
pub(crate) async fn open_staging(
    run_dir: &Path,
    node: &NodeId,
    opening: Opening,
) -> std::io::Result<PathBuf> {
    let dir = staging(run_dir, node);
    if opening == Opening::Fresh {
        match tokio::fs::remove_dir_all(&dir).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    tokio::fs::create_dir_all(&dir).await?;
    Ok(dir)
}
