//! What the terminal this process was handed allows it to draw: read
//! once from the process, then passed as a value, so every policy that
//! decides what to draw is a function of its arguments.

use std::io::IsTerminal;

/// The two values the delivery policy reads, lifted out of the process
/// so the policy is a function of its arguments and nothing else.
pub(crate) struct TerminalEnv {
    /// Whether the stream the region would draw on is a terminal.
    pub(crate) stderr_is_terminal: bool,
    /// `TERM`.
    pub(crate) term: Option<String>,
}

impl TerminalEnv {
    /// What this process was started with.
    pub(crate) fn from_process() -> Self {
        Self {
            stderr_is_terminal: std::io::stderr().is_terminal(),
            term: std::env::var("TERM").ok(),
        }
    }
}
