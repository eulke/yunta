//! The scripted CLIs the adapter and CLI suites drive in place of a real
//! `codex` or `claude`: no network, no API cost, deterministic.
//!
//! They live here because the two suites that drive them sit in
//! different crates — `yunta-adapters` tests the adapter that spawns
//! them, and `yunta`'s tests drive a whole run through one — and this is
//! the crate both can reach. Each script honours the same knobs under
//! its own prefix; the two below are what a test asking how a session
//! dies reaches for:
//!
//! - `<NAME>_STUB_STDERR`: written to stderr before the session streams
//!   anything, which is what a CLI that refuses its configuration does.
//! - `<NAME>_STUB_EXIT`: the status to exit with.

use std::path::PathBuf;

/// The fake `codex` binary, by absolute path. Its prefix is `CODEX_STUB`.
pub fn codex() -> PathBuf {
    stub("codex_stub.sh")
}

/// The fake `claude` binary, by absolute path. Its prefix is
/// `CLAUDE_STUB`.
pub fn claude_code() -> PathBuf {
    stub("claude_code_stub.sh")
}

fn stub(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("stubs")
        .join(name)
}
