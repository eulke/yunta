//! What a `codex` session's fence is built of, and what that leaves
//! covered.
//!
//! Every file edit Codex makes is an `apply_patch`, and the hook it runs
//! before one asks the engine about every path the patch names — that
//! channel is exact. Commands the session runs are confined by the
//! process's own sandbox instead, by directory and never by glob: the
//! coverage says how wide that left them. The post-check diff is what
//! catches a command's write inside those directories but outside the
//! declared scope.

use std::path::{Path, PathBuf};

use yunta_core::fence::{Coverage, Fence, Fenced};
use yunta_core::port::PermissionProfile;

use super::settings::Sandbox;

/// The sandbox flags one session runs under.
pub(super) fn sandbox_args(
    fence: &Fence,
    profile: PermissionProfile,
    edit_sandbox: Sandbox,
) -> Vec<String> {
    vec![
        "--sandbox".to_string(),
        mode(fence, profile, edit_sandbox).as_flag().to_string(),
    ]
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

/// What this session ends up fenced by: its edits exactly, and its
/// commands by whatever the sandbox under `mode` confines them to — the
/// workspace and the roots, nothing at all, or everything.
pub(super) fn coverage(fence: &Fence, cwd: &Path, mode: Sandbox) -> Coverage {
    let commands = match mode {
        Sandbox::WorkspaceWrite => Some(Fenced::Roots(
            std::iter::once(cwd.to_path_buf())
                .chain(fence.roots.iter().cloned())
                .collect::<Vec<PathBuf>>(),
        )),
        Sandbox::ReadOnly => Some(Fenced::Exact),
        Sandbox::DangerFullAccess => None,
    };
    Coverage::of(Fenced::Exact, commands)
}
