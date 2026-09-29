//! What the terminal this process was handed allows it to draw: read
//! once from the process, then passed as a value, so every policy that
//! decides what to draw is a function of its arguments.

use std::io::IsTerminal;

use crate::render::ColorPolicy;

/// The three values the delivery policy reads, lifted out of the process
/// so the policy is a function of its arguments and nothing else.
pub(crate) struct TerminalEnv {
    /// Whether the stream the region would draw on is a terminal.
    pub(crate) stderr_is_terminal: bool,
    /// `TERM`.
    pub(crate) term: Option<String>,
    /// `NO_COLOR`, kept as an option so color policy can distinguish an
    /// unset variable from an empty but defined value.
    pub(crate) no_color: Option<String>,
}

impl TerminalEnv {
    /// What this process was started with.
    pub(crate) fn from_process() -> Self {
        Self {
            stderr_is_terminal: std::io::stderr().is_terminal(),
            term: std::env::var("TERM").ok(),
            no_color: std::env::var_os("NO_COLOR")
                .map(|value| value.to_string_lossy().into_owned()),
        }
    }

    /// The color policy for stderr, the stream used by common messages.
    /// A non-terminal, `TERM=dumb`, or any defined `NO_COLOR` value keeps
    /// those messages readable without ANSI sequences.
    pub(crate) fn color_policy(&self) -> ColorPolicy {
        ColorPolicy::for_stream(
            self.stderr_is_terminal && self.term.as_deref() != Some("dumb"),
            self.no_color.as_deref(),
        )
    }
}
