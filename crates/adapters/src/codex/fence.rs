//! The sandbox a `codex` session runs under, and the directories that
//! sandbox keeps writable beside the workspace.
//!
//! Every file edit Codex makes is an `apply_patch`, and the hook it runs
//! before one asks the engine about every path the patch names — that
//! channel is exact. Commands the session runs are confined by the process's
//! own sandbox instead, by directory and never by glob; the coverage a
//! session reports says how wide that left them.

use yunta_core::fence::Fence;
use yunta_core::port::PermissionProfile;

use crate::session::Sandbox;

/// The sandbox flags one session runs under.
pub(super) fn sandbox_args(sandbox: Sandbox) -> Vec<String> {
    vec!["--sandbox".to_string(), sandbox.as_flag().to_string()]
}

/// The sandbox mode a session under `profile` runs in.
///
/// A `read_only` session that must still write the run's roots runs under
/// `workspace-write`: `read-only` seals the roots along with everything
/// else, whatever `--add-dir` says, and the hook refuses every edit that
/// is not one of them.
pub(super) fn mode(fence: &Fence, profile: PermissionProfile, edit_sandbox: Sandbox) -> Sandbox {
    match profile {
        PermissionProfile::ReadOnly if fence.roots.is_empty() => Sandbox::ReadOnly,
        PermissionProfile::ReadOnly => Sandbox::WorkspaceWrite,
        PermissionProfile::Edit => edit_sandbox,
        PermissionProfile::Full => Sandbox::DangerFullAccess,
    }
}

/// The directories the sandbox keeps writable beside the workspace.
pub(super) fn writable_roots(fence: &Fence) -> Vec<String> {
    fence
        .roots
        .iter()
        .map(|root| root.display().to_string())
        .collect()
}
