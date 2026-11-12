//! The one error a command hands back and the one place its text reaches
//! a person. Every subcommand returns a `Result` of an
//! [`Outcome`](crate::output::Outcome) or a [`CliError`]; `main` prints a
//! [`CliError`] once, on stderr, and maps the exit code. A command that ran
//! but reports a failing verdict (an invalid workflow, a test case that
//! failed) returns [`Outcome::Reported`](crate::output::Outcome::Reported)
//! instead — its detail is already on the command's own output, and no error
//! banner belongs on top of it.

use std::fmt::Display;

use yunta_render::command::Reader;
use yunta_render::refusal::Refused;

pub(crate) mod page;

/// Everything a subcommand can fail with, phrased so its `Display` names
/// what went wrong and what to do. `main` turns it into one line on
/// stderr and a failing exit code — the only place the CLI writes an
/// error.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// A filesystem or process step the command needed failed; `context`
    /// says what it was doing so the message names the fix.
    #[error("failed to {context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    /// The current directory could not be read — every command that
    /// resolves a project needs it, so it fails before doing anything.
    #[error("cannot determine the current directory: {source}")]
    Cwd {
        #[source]
        source: std::io::Error,
    },

    /// What failed below the API, said in its own words and its causes'.
    #[error(transparent)]
    Failed(#[from] yunta_api::Failure),

    /// This invocation's stop: a person interrupted it, said as a run the
    /// engine was stopped in says it.
    #[error("interrupted by a person")]
    Stopped,

    /// A value that has to be an identifier and is not — a run id, an
    /// adapter, a mode, a gate option, a responder — wherever one is
    /// read off an argument.
    #[error(transparent)]
    InvalidId(#[from] yunta_core::InvalidId),

    #[error(transparent)]
    FrozenPaths(#[from] yunta_core::RelativeRootError),

    /// What Yunta refused: render says it, to a person and to an agent,
    /// with the commands that go on from it.
    #[error(transparent)]
    Refused(#[from] Refused),

    /// A condition specific to one command, already phrased as an
    /// actionable message at the point it is detected — the CLI's own
    /// border for something no shared type names.
    ///
    /// Exceptional, and meant to stay that way: a failure two commands
    /// can reach, or one a caller has to tell apart from another, earns
    /// an arm of its own. A `String` here is a failure that has exactly
    /// one site and nothing to match on.
    #[error("{0}")]
    Message(String),
}

impl CliError {
    /// Builds an [`CliError::Io`] whose `context` is `verb`ing `subject`
    /// (e.g. `io("write", path.display())`).
    pub fn io(verb: &str, subject: impl Display, source: std::io::Error) -> Self {
        CliError::Io {
            context: format!("{verb} {subject}"),
            source,
        }
    }

    /// A one-off actionable message no shared error type names.
    pub fn msg(message: impl Into<String>) -> Self {
        CliError::Message(message.into())
    }
}

/// What the API did not do: a refusal render says, a failure below it, or
/// this invocation's own stop.
impl From<yunta_api::Error> for CliError {
    fn from(error: yunta_api::Error) -> Self {
        match error {
            yunta_api::Error::Refused(refused) => CliError::Refused(refused),
            yunta_api::Error::Failed(failure) => CliError::Failed(failure),
            yunta_api::Error::Stopped => CliError::Stopped,
        }
    }
}

/// A document this binary does not show is refused, naming the ones it does.
impl From<yunta_core::UnknownDocument> for CliError {
    fn from(unknown: yunta_core::UnknownDocument) -> Self {
        Refused::UnknownDocument(unknown).into()
    }
}

impl CliError {
    /// The refusal as `reader` reads it, with what caused it: a run's
    /// advice names it by its handle for a person, by its whole id for
    /// an agent. A refusal that found problems in a text says how many to
    /// a person, whose page points at each in its line, and says each to
    /// an agent, which reads no page.
    pub fn said_to(&self, reader: Reader) -> String {
        let said = match self {
            CliError::Refused(refused) => refused.said(reader),
            failed => failed.to_string(),
        };
        // What caused it goes with it, to a person and an agent alike: a
        // refusal that stops at its own sentence hides why.
        yunta_core::with_causes(said, self)
    }

    /// What `reader` is told beside the refusal: where it looked, and the
    /// commands that go on from here. Empty for a refusal that says it
    /// all in its sentence.
    pub fn advice(&self, reader: Reader) -> (Option<String>, Vec<(String, &'static str)>) {
        match self {
            CliError::Refused(refused) => refused.advice(reader),
            _ => (None, Vec::new()),
        }
    }
}

/// `warning`, as stderr is painted.
pub(crate) fn warning_word() -> String {
    crate::output::stderr_paint().word(surco::line::Tone::Caution, "warning")
}
