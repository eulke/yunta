//! Maps yunta's portable `PermissionProfile` to the real `claude` CLI's
//! flags (T7.3) — the CLI-specific knowledge A1 says belongs here, not
//! in the engine.
//!
//! Headless execution has no human to answer a permission prompt, so
//! every profile must resolve to a non-interactive mode or the process
//! hangs forever. `ReadOnly` gets there by restricting the tool set to
//! non-mutating tools and then bypassing prompts safely — there is
//! nothing mutating left to bypass. `Edit` and `Full` both need
//! `--dangerously-skip-permissions` to run unattended and are, today,
//! indistinguishable at the CLI level: the real boundary in M-0 is the
//! engine's own post-hoc scope check (T5.3), not a live CLI restriction
//! — `capabilities().edit_hooks` says so honestly (`false`, see mod.rs).

use crate::session::PermissionProfile;

pub(super) fn permission_args(profile: PermissionProfile) -> Vec<String> {
    match profile {
        PermissionProfile::ReadOnly => vec![
            "--tools".to_string(),
            "Read,Grep,Glob,WebFetch,WebSearch".to_string(),
            "--permission-mode".to_string(),
            "bypassPermissions".to_string(),
        ],
        PermissionProfile::Edit | PermissionProfile::Full => {
            vec!["--dangerously-skip-permissions".to_string()]
        }
    }
}
