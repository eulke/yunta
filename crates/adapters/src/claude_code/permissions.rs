//! Maps yunta's portable `PermissionProfile` to the real `claude` CLI's
//! flags — CLI-specific knowledge that belongs here, not in the engine.
//!
//! Headless execution has no human to answer a permission prompt, so
//! every profile must resolve to a mode that neither hangs nor silently
//! blocks the work. Three modes were tried empirically against a live
//! session before landing on this one:
//! - `bypassPermissions`/`--dangerously-skip-permissions`: refused
//!   outright when the CLI runs as root (`cannot be used with root/sudo
//!   privileges for security reasons`) — exactly the case a
//!   containerized worktree runs as.
//! - `dontAsk`: runs unattended, but *denies every tool call* rather
//!   than allowing it — the name means "don't ask, don't do it either,"
//!   confirmed by a live session whose Write and Bash calls both came
//!   back "Permission ... denied because Claude Code is running in
//!   don't ask mode."
//! - `acceptEdits`: confirmed live to auto-accept both Write and Bash
//!   calls unattended, runs as root, and actually produces the file a
//!   prompt asked for. This is what every profile uses.
//!
//! Each profile is a distinct tool set, which is what makes the
//! `permission_profiles` capability true: `ReadOnly` gets the
//! non-mutating tools and a shell that writes nothing of the checkout,
//! only temp and the run's roots; `Edit` gets the file-editing tools and a
//! shell confined to the workspace, temp and the roots; `Full` gets the
//! CLI's whole tool set and an unconfined shell. Where a file tool writes
//! is the fence: every writing tool goes through the hook that asks the
//! engine before the write happens. Where a command writes, and whether it
//! reaches the network, is the CLI's own sandbox, enforced by the operating
//! system.

use std::path::{Path, PathBuf};

use yunta_core::port::PermissionProfile;
use yunta_core::{temp_dirs, Commands, Confinement, Unbuildable};

/// The tools a profile allows, as the CLI names them; `None` leaves the
/// CLI's whole tool set available.
///
/// `ReadOnly` keeps `Write` and `Edit` only when the session has
/// somewhere declared to write: a profile says what the session may do
/// to *the project*, and a node's declared file is not the project —
/// it is the node's own output, written to a root the confinement keeps
/// writable. With no such root, a read-only session has nothing at all
/// to write and the tools go with it.
pub(super) fn tools(profile: PermissionProfile, has_declared_files: bool) -> Option<&'static str> {
    match profile {
        PermissionProfile::ReadOnly if has_declared_files => {
            Some("Read,Grep,Glob,WebFetch,WebSearch,Bash,Write,Edit")
        }
        PermissionProfile::ReadOnly => Some("Read,Grep,Glob,WebFetch,WebSearch,Bash"),
        PermissionProfile::Edit => Some("Read,Grep,Glob,Bash,Edit,Write,MultiEdit,NotebookEdit"),
        PermissionProfile::Full => None,
    }
}

/// The CLI's `sandbox` setting for a session's commands under `conf`: what
/// they may write and reach. It is `None` only when the commands run
/// unconfined with the network open, where the CLI takes no setting. The
/// sandbox fails the session rather than run unconfined, and never lets a
/// command leave it: a command writes the working directory unless the
/// session denies it, and the roots and temp directories, and it reaches
/// the network, loopback included, only where the confinement says so.
pub(super) fn settings(
    conf: &Confinement,
    cwd: &Path,
) -> Result<Option<serde_json::Value>, Unbuildable> {
    if conf.commands == Commands::Unconfined {
        return if conf.network {
            Ok(None)
        } else {
            Err(Unbuildable::NetworkUnclosable)
        };
    }
    let network = if conf.network {
        serde_json::json!({ "allowLocalBinding": true, "allowedDomains": ["*"] })
    } else {
        serde_json::json!({})
    };
    // The trees the run protects are denied in every session, and a session
    // that writes nothing of the checkout has the checkout itself denied.
    let mut deny = conf.protected_trees(cwd);
    if !conf.writes_worktree() {
        deny.push(cwd.to_path_buf());
    }
    let allow: Vec<PathBuf> = conf.roots.iter().cloned().chain(temp_dirs()).collect();
    Ok(Some(serde_json::json!({
        "enabled": true,
        "failIfUnavailable": true,
        "allowUnsandboxedCommands": false,
        "network": network,
        "filesystem": { "allowWrite": allow, "denyWrite": deny },
    })))
}

pub(super) fn permission_args(profile: PermissionProfile, has_declared_files: bool) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(tools) = tools(profile, has_declared_files) {
        args.push("--tools".to_string());
        args.push(tools.to_string());
    }
    args.push("--permission-mode".to_string());
    args.push("acceptEdits".to_string());
    args
}
