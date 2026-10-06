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
//! non-mutating tools and a shell that writes nothing but the run's
//! roots; `Edit` gets the file-editing tools and a shell confined to the
//! workspace and the roots; `Full` gets the CLI's whole tool set and an
//! unconfined shell. Where a file tool writes is the fence: every writing
//! tool goes through the hook that asks the engine before the write
//! happens. Where a command writes, and whether it reaches the network, is
//! the CLI's own sandbox, enforced by the operating system.

use std::path::{Path, PathBuf};

use yunta_core::fence::Fenced;
use yunta_core::port::PermissionProfile;

use crate::session::Sandbox;

/// The tools a profile allows, as the CLI names them; `None` leaves the
/// CLI's whole tool set available.
///
/// `ReadOnly` keeps `Write` and `Edit` only when the session has
/// somewhere declared to write: a profile says what the session may do
/// to *the project*, and a node's declared file is not the project —
/// it is the node's own output, written to a root the fence keeps
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

/// What a session's commands are left fenced by under `mode`: nothing
/// written but the roots, the workspace and the roots, or anything.
pub(super) fn other_channels(mode: Sandbox, cwd: &Path, roots: &[PathBuf]) -> Option<Fenced> {
    match mode {
        Sandbox::ReadOnly if roots.is_empty() => Some(Fenced::Exact),
        Sandbox::ReadOnly => Some(Fenced::Roots(roots.to_vec())),
        Sandbox::WorkspaceWrite => Some(Fenced::Roots(
            std::iter::once(cwd.to_path_buf())
                .chain(roots.iter().cloned())
                .collect(),
        )),
        Sandbox::DangerFullAccess => None,
    }
}

/// The CLI's `sandbox` settings for a session's commands under `mode`, or
/// `None` for one left unconfined. The sandbox fails the session rather
/// than run unconfined, and never lets a command leave it. A command
/// writes the working directory — except under `read-only` — and the
/// roots the CLI was handed with `--add-dir`; it reaches the network,
/// loopback included, only where `network` says so and the mode lets it.
pub(super) fn sandbox(mode: Sandbox, cwd: &Path, network: bool) -> Option<serde_json::Value> {
    let network = match (mode, network) {
        (Sandbox::WorkspaceWrite, true) => serde_json::json!({
            "allowLocalBinding": true,
            "allowedDomains": ["*"],
        }),
        _ => serde_json::json!({}),
    };
    let mut settings = serde_json::Map::from_iter([
        ("enabled".to_string(), serde_json::Value::Bool(true)),
        (
            "failIfUnavailable".to_string(),
            serde_json::Value::Bool(true),
        ),
        (
            "allowUnsandboxedCommands".to_string(),
            serde_json::Value::Bool(false),
        ),
        ("network".to_string(), network),
    ]);
    match mode {
        Sandbox::DangerFullAccess => return None,
        Sandbox::ReadOnly => {
            settings.insert(
                "filesystem".to_string(),
                serde_json::json!({ "denyWrite": [cwd] }),
            );
        }
        Sandbox::WorkspaceWrite => {}
    }
    Some(serde_json::Value::Object(settings))
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
