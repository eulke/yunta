//! Maps yunta's portable `PermissionProfile` to `codex exec`'s own
//! `-s`/`--sandbox` modes (T7.4) — the CLI-specific knowledge A1 says
//! belongs here, not in the engine. Confirmed sandbox mode values
//! (openai/codex): `read-only`, `workspace-write`, `danger-full-access`.
//!
//! Unlike `claude_code::permissions`, this mapping has **no live
//! confirmation** behind it — this module's own crate doc explains why
//! (no `codex` binary or credentials in this environment). `codex exec`
//! is documented as non-interactive by construction (approval defaults
//! to "never" in exec mode, distinct from the interactive CLI), so
//! there is no separate "don't hang waiting for a permission prompt"
//! flag to add the way `claude_code`'s `acceptEdits` was needed for —
//! the sandbox flag alone is what each profile maps to.

use crate::session::PermissionProfile;

pub(super) fn sandbox_args(profile: PermissionProfile) -> Vec<String> {
    let mode = match profile {
        PermissionProfile::ReadOnly => "read-only",
        PermissionProfile::Edit => "workspace-write",
        PermissionProfile::Full => "danger-full-access",
    };
    vec!["--sandbox".to_string(), mode.to_string()]
}
