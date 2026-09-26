//! What the `codex` sandbox can build of a session's fence, and what
//! that leaves covered.
//!
//! `codex exec` confines writes with a filesystem sandbox: a set of
//! writable directories, no globs. So the fence it builds is wider than
//! the one the run declared — exactly the worktree plus the roots — and
//! the coverage says so. The post-check diff is what catches a write
//! inside those directories but outside the declared scope.

use std::path::{Path, PathBuf};

use yunta_core::fence::{Coverage, Fence, Fenced};
use yunta_core::port::PermissionProfile;
use yunta_core::Unbuildable;

use super::settings::Sandbox;

/// The sandbox flags one session runs under: the mode its profile maps
/// to, and every directory outside the workspace the fence keeps
/// writable.
///
/// A `read_only` profile with roots to keep writable cannot be built at
/// all: the sandbox has one setting for the whole filesystem, and
/// `read-only` seals the roots along with everything else. A capability
/// that is absent fails rather than spending a session that could never
/// write what the node declared.
pub(super) fn sandbox_args(
    fence: &Fence,
    profile: PermissionProfile,
    edit_sandbox: Sandbox,
) -> Result<Vec<String>, Unbuildable> {
    if profile == PermissionProfile::ReadOnly && !fence.roots.is_empty() {
        return Err(Unbuildable::SealedRoots(fence.roots.clone()));
    }
    let mode = match profile {
        PermissionProfile::ReadOnly => Sandbox::ReadOnly,
        PermissionProfile::Edit => edit_sandbox,
        PermissionProfile::Full => Sandbox::DangerFullAccess,
    };
    Ok(vec!["--sandbox".to_string(), mode.as_flag().to_string()])
}

/// The directories the sandbox keeps writable beside the workspace.
pub(super) fn writable_roots(fence: &Fence) -> Vec<String> {
    fence
        .roots
        .iter()
        .map(|root| root.display().to_string())
        .collect()
}

/// What this session ends up fenced by: the same directories in both
/// channels, because the sandbox is the process's, not the tool's.
pub(super) fn coverage(fence: &Fence, cwd: &Path) -> Coverage {
    let directories: Vec<PathBuf> = std::iter::once(cwd.to_path_buf())
        .chain(fence.roots.iter().cloned())
        .collect();
    Coverage::of(
        Fenced::Roots(directories.clone()),
        Some(Fenced::Roots(directories)),
    )
}
