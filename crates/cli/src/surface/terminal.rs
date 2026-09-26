//! What the terminal this process was handed allows it to draw: read
//! once from the process, then passed as a value, so every policy that
//! decides what to draw is a function of its arguments.

use std::io::IsTerminal;

/// The three values the delivery policy reads, lifted out of the process
/// so the policy is a function of its arguments and nothing else.
pub(crate) struct TerminalEnv {
    /// Whether the stream the region would draw on is a terminal.
    pub(crate) stderr_is_terminal: bool,
    /// `TERM`.
    pub(crate) term: Option<String>,
    /// `NO_COLOR`, which the convention reads as set when it is present
    /// and not empty.
    pub(crate) no_color: Option<String>,
}

impl TerminalEnv {
    /// What this process was started with.
    pub(crate) fn from_process() -> Self {
        Self {
            stderr_is_terminal: std::io::stderr().is_terminal(),
            term: std::env::var("TERM").ok(),
            no_color: std::env::var("NO_COLOR").ok(),
        }
    }

    /// Whether a line on stderr may carry color: the same three signals
    /// [`super::Delivery::choose`] reads, since a stream that is not a terminal,
    /// a terminal that declares itself dumb and a reader who asked for no
    /// color each rule it out.
    pub(crate) fn draws_color(&self) -> bool {
        self.stderr_is_terminal
            && self.term.as_deref() != Some("dumb")
            && !self.no_color.as_deref().is_some_and(|set| !set.is_empty())
    }
}
