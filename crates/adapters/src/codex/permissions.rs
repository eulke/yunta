//! Maps yunta's portable `PermissionProfile` to `codex exec`'s own
//! `-s`/`--sandbox` modes — CLI-specific knowledge that belongs here,
//! not in the engine.
//!
//! The three string values (`read-only`, `workspace-write`,
//! `danger-full-access`) are triangulated, not read off one source:
//! they match `SandboxPolicy`'s variants in `codex-rs/core` (`ReadOnly`
//! / `WorkspaceWrite`/`DangerFullAccess`, kebab-cased), a gist of 81
//! empirically-tested `codex exec` invocations that used them directly
//! on the real CLI, and several `openai/codex` bug reports whose own
//! repro commands pass these same literal strings to `--sandbox`.
//!
//! Unlike `claude_code::permissions`, this mapping has **no live
//! confirmation from this session** behind it — this crate's own
//! top-level doc (`mod.rs`) explains why (no `codex` binary or
//! credentials in this environment). `codex exec` is documented as
//! non-interactive by construction (approval defaults to "never" in
//! exec mode, distinct from the interactive CLI), so there is no
//! separate "don't hang waiting for a permission prompt" flag to add
//! the way `claude_code`'s `acceptEdits` was needed for — the sandbox
//! flag alone is what each profile maps to.

use super::settings::Sandbox;
use crate::session::PermissionProfile;

/// `edit_sandbox` is the mode the `Edit` profile runs under — the
/// adapter's `sandbox` setting, `workspace-write` by default.
pub(super) fn sandbox_args(profile: PermissionProfile, edit_sandbox: Sandbox) -> Vec<String> {
    let mode = match profile {
        PermissionProfile::ReadOnly => Sandbox::ReadOnly,
        PermissionProfile::Edit => edit_sandbox,
        PermissionProfile::Full => Sandbox::DangerFullAccess,
    };
    vec!["--sandbox".to_string(), mode.as_flag().to_string()]
}
