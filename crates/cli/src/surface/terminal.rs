//! What the terminal this process was handed allows it to draw: read
//! once from the process, then passed as a value, so every policy that
//! decides what to draw is a function of its arguments.

use std::io::IsTerminal;
use std::sync::OnceLock;

/// The two values the delivery policy reads, lifted out of the process
/// so the policy is a function of its arguments and nothing else.
pub(crate) struct TerminalEnv {
    /// Whether the stream the region would draw on is a terminal.
    pub(crate) stderr_is_terminal: bool,
    /// `TERM`.
    pub(crate) term: Option<String>,
}

static SETTLED: OnceLock<TerminalEnv> = OnceLock::new();

/// Fixes what this process's terminal allows, from what it was started
/// with. `main` calls it once, before anything is printed; a later call
/// changes nothing.
pub(crate) fn settle() {
    SETTLED.get_or_init(|| TerminalEnv {
        stderr_is_terminal: std::io::stderr().is_terminal(),
        term: std::env::var("TERM").ok(),
    });
}

impl TerminalEnv {
    /// What `main` settled, or no terminal at all in a process that
    /// never settled one — a unit test, which draws nothing live.
    pub(crate) fn settled() -> &'static Self {
        SETTLED.get_or_init(|| TerminalEnv {
            stderr_is_terminal: false,
            term: None,
        })
    }
}
