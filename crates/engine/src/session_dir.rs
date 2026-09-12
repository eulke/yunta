//! Which session of a run a set of files on disk belongs to.

use std::path::{Path, PathBuf};

use yunta_core::{NodeId, TaskId};

/// One session of a run, named in the run's own terms — the identity
/// the engine holds before a CLI mints a session id of its own.
///
/// It names the session's private scratch directory. Sessions that can
/// be alive at the same moment never share one, so a file an adapter
/// drops for a session — an MCP config carrying that session's own
/// bearer token, say — is never a file another session reads. Deriving
/// the directory from identity the engine already has is what keeps a
/// name from having to be parsed back out of a transport detail.
pub(crate) enum SessionSlot<'a> {
    /// A node's own session.
    Node(&'a NodeId),
    /// One task attempt inside a loop node. Siblings run concurrently,
    /// which is what makes the task's own id part of the name.
    Task(&'a NodeId, &'a TaskId),
}

impl SessionSlot<'_> {
    /// This session's own directory under the run's `scratch/`.
    ///
    /// Every segment is a checked identifier — ASCII alphanumerics,
    /// `_`, `-`, and `@` between a fan-out sibling's two halves — so a
    /// name can neither escape the run directory nor collide with the
    /// engine's own files beside it.
    pub(crate) fn scratch_dir(&self, run_dir: &Path) -> PathBuf {
        let sessions = run_dir.join("scratch").join("sessions");
        match self {
            Self::Node(node) => sessions.join(node.as_str()),
            Self::Task(node, task) => sessions.join(node.as_str()).join(task.as_str()),
        }
    }
}
