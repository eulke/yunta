//! Runtime permission enforcement, pure half.
//!
//! The permissions model matches patterns over hook/criterion/bash/
//! executor commands, checked right before execution. Three example
//! patterns (`"sudo *"`, `"curl * | *"`) motivate the shape without
//! fully defining the matching dialect. The dialect implemented here:
//!
//! - A pattern matches the **whole** command string, anchored at both
//!   ends: `sudo *` blocks `sudo rm` and never `echo sudo` — a mention is
//!   not an escalation.
//! - `*` matches any run of characters, spaces, pipes and newlines
//!   included — `curl * | *` needs that to mean "curl piped anywhere".
//!   Everything else is literal; case-sensitive; no character classes,
//!   no `?` — governance patterns stay boring and predictable.
//! - `deny` wins over `allow`. An empty `allow` is denylist mode
//!   (everything not denied runs, the default); a non-empty `allow`
//!   is a strict allowlist — the command must match one.
//!
//! The honest limit: this is governance, not a sandbox. A
//! write-capable agent can route around a textual pattern by writing a
//! script and running it — the model stops accidents and careless packs,
//! and leaves an auditable trail of the deliberate attempt.

use yunta_core::PermissionsConfig;

/// Checks one rendered command against the effective (ceiling-merged)
/// permissions. `None` = allowed; `Some(rule)` = blocked, with a
/// diagnostic that cites the rule, ready to become the
/// node's `node_failed.outcome`.
pub fn command_violation(command: &str, permissions: Option<&PermissionsConfig>) -> Option<String> {
    let commands = permissions?.commands.as_ref()?;

    for pattern in &commands.deny {
        if glob_match(pattern, command) {
            return Some(format!(
                "command `{command}` matches denied pattern `{pattern}` (permissions.commands.deny)"
            ));
        }
    }

    if !commands.allow.is_empty()
        && !commands
            .allow
            .iter()
            .any(|pattern| glob_match(pattern, command))
    {
        return Some(format!(
            "command `{command}` matches no pattern in the strict allowlist (permissions.commands.allow)"
        ));
    }

    None
}

/// Full-string glob: `*` matches any (possibly empty) run of characters —
/// newlines included — and every other byte is literal. Iterative
/// backtracking over bytes (patterns and commands are treated as raw
/// text; `*` boundaries never split the middle of a UTF-8 sequence in a
/// way that affects matching, since matching is byte-exact).
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern = pattern.as_bytes();
    let text = text.as_bytes();

    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;

    while t < text.len() {
        if p < pattern.len() && pattern[p] == b'*' {
            star = Some((p, t));
            p += 1;
        } else if p < pattern.len() && pattern[p] == text[t] {
            p += 1;
            t += 1;
        } else if let Some((star_p, star_t)) = star {
            // Backtrack: let the last `*` swallow one more byte.
            p = star_p + 1;
            t = star_t + 1;
            star = Some((star_p, star_t + 1));
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}
