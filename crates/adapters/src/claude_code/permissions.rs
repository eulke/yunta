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
//! `ReadOnly` additionally restricts the tool set to non-mutating tools,
//! so "accept edits" has nothing mutating to accept. `Edit` and `Full`
//! are, today, indistinguishable at the CLI level: the real boundary is
//! the engine's own post-hoc scope check, not a live CLI restriction —
//! `capabilities().edit_hooks` says so honestly (`false`, see mod.rs).

use crate::session::PermissionProfile;

pub(super) fn permission_args(profile: PermissionProfile) -> Vec<String> {
    let mut args = match profile {
        PermissionProfile::ReadOnly => vec![
            "--tools".to_string(),
            "Read,Grep,Glob,WebFetch,WebSearch".to_string(),
        ],
        PermissionProfile::Edit | PermissionProfile::Full => Vec::new(),
    };
    args.push("--permission-mode".to_string());
    args.push("acceptEdits".to_string());
    args
}
