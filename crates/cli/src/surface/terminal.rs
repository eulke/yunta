//! What the terminal this process was handed allows it to draw: read
//! once from the process, then passed as a value, so every policy that
//! decides what to draw is a function of its arguments.

use std::io::IsTerminal;
use std::sync::OnceLock;

use surco::motion::Motion;
use yunta_core::Env;

/// The values the delivery policy reads, lifted out of the process so the
/// policy is a function of its arguments and nothing else.
pub(crate) struct TerminalEnv {
    /// Whether the stream the region would draw on is a terminal.
    pub(crate) stderr_is_terminal: bool,
    /// `TERM`.
    pub(crate) term: Option<String>,
    /// Whether a live view moves, as the reader asked in `YUNTA_MOTION`.
    pub(crate) motion: Motion,
}

static SETTLED: OnceLock<TerminalEnv> = OnceLock::new();

/// Fixes what this process's terminal allows, from the environment it was
/// started with. `main` calls it once, ahead of any output; any
/// further call changes nothing.
pub(crate) fn settle(env: &Env) {
    SETTLED.get_or_init(|| TerminalEnv {
        stderr_is_terminal: std::io::stderr().is_terminal(),
        term: env.terminal_var("TERM").map(str::to_string),
        motion: Motion::of(env.terminal_var("YUNTA_MOTION")),
    });
}

impl TerminalEnv {
    /// What `main` settled, or no terminal at all in a process that
    /// never settled one — a unit test, which draws nothing live.
    pub(crate) fn settled() -> &'static Self {
        SETTLED.get_or_init(|| TerminalEnv {
            stderr_is_terminal: false,
            term: None,
            motion: Motion::default(),
        })
    }

    /// Whether the terminal declares itself dumb: it has said it draws nothing
    /// beyond text.
    pub(crate) fn is_dumb(&self) -> bool {
        self.term.as_deref() == Some("dumb")
    }
}
