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
//! non-mutating tools, so "accept edits" has nothing mutating to
//! accept; `Edit` gets the file-editing tools and nothing that reaches a
//! shell or the network; `Full` gets the CLI's whole tool set. Where a
//! file lands is the fence: every writing tool goes through the hook
//! that runs the judge before the write happens.

use yunta_core::fence::Fenced;
use yunta_core::port::PermissionProfile;

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
            Some("Read,Grep,Glob,WebFetch,WebSearch,Write,Edit")
        }
        PermissionProfile::ReadOnly => Some("Read,Grep,Glob,WebFetch,WebSearch"),
        PermissionProfile::Edit => Some("Read,Grep,Glob,Edit,Write,MultiEdit,NotebookEdit"),
        PermissionProfile::Full => None,
    }
}

/// What each profile leaves fenced outside the file tools. `edit` and
/// `read_only` expose no shell at all, so there is no second channel to
/// widen the coverage; `full` does, and nothing fences a shell by path.
pub(super) fn other_channels(profile: PermissionProfile) -> Option<Fenced> {
    match profile {
        PermissionProfile::ReadOnly | PermissionProfile::Edit => Some(Fenced::Exact),
        PermissionProfile::Full => None,
    }
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
