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
//! file lands is still the engine's own post-hoc scope check, not a
//! live CLI restriction — `capabilities().edit_hooks` says so (`false`,
//! see mod.rs).

use crate::session::PermissionProfile;

/// The tools a profile allows, as the CLI names them; `None` leaves the
/// CLI's whole tool set available.
/// `Write` is in every profile, `ReadOnly` included, because a profile
/// says what the session may do to *the project*, and a node's declared
/// artifact is not the project: it is the node's own output, written to
/// the run directory, which `--add-dir` is what actually opens. Leaving
/// `Write` out of `ReadOnly` made a read-only node that declares an
/// artifact unable to produce one — the node then failed at close for a
/// file its session was never permitted to create.
pub(super) fn tools(profile: PermissionProfile) -> Option<&'static str> {
    match profile {
        PermissionProfile::ReadOnly => Some("Read,Grep,Glob,WebFetch,WebSearch,Write"),
        PermissionProfile::Edit => Some("Read,Grep,Glob,Edit,Write,MultiEdit,NotebookEdit"),
        PermissionProfile::Full => None,
    }
}

pub(super) fn permission_args(profile: PermissionProfile) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(tools) = tools(profile) {
        args.push("--tools".to_string());
        args.push(tools.to_string());
    }
    args.push("--permission-mode".to_string());
    args.push("acceptEdits".to_string());
    args
}
