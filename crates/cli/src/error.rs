//! The one error a command hands back and the one place its text reaches
//! a person. Every subcommand returns [`Result<Outcome, CliError>`]; `main`
//! prints a [`CliError`] once, on stderr, and maps the exit code. A
//! command that ran but reports a failing verdict (an invalid workflow, a
//! test case that failed) returns [`Outcome::Reported`] instead — its
//! detail is already on the command's own output, and no error banner
//! belongs on top of it.

use std::fmt::Display;
use std::path::Path;

/// How a subcommand came back when nothing stopped it from running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The command did its job; exit 0.
    Success,
    /// The command ran and reports a failing verdict — the detail is
    /// already on its output. Exit non-zero, no error banner.
    Reported,
}

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

    #[error(transparent)]
    Project(#[from] crate::project::ProjectError),

    #[error(transparent)]
    Pack(#[from] crate::pack::PackError),

    #[error(transparent)]
    Storage(#[from] yunta_storage::StorageError),

    #[error(transparent)]
    Catalog(#[from] yunta_engine::CatalogError),

    #[error(transparent)]
    Run(#[from] yunta_engine::RunError),

    #[error(transparent)]
    ManifestRead(#[from] yunta_engine::ManifestReadError),

    #[error(transparent)]
    Manifest(#[from] yunta_engine::ManifestError),

    #[error(transparent)]
    ResolveGate(#[from] yunta_engine::ResolveGateError),

    #[error(transparent)]
    Worktree(#[from] yunta_engine::WorktreeError),

    /// A condition specific to one command, already phrased as an
    /// actionable message at the point it is detected — the CLI's own
    /// border for something no shared type names.
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

/// A warning to stderr — the one place the CLI prints `warning:` lines,
/// so a run that succeeds with caveats still says so without an error.
pub fn warn(message: impl Display) {
    eprintln!("warning: {message}");
}

/// An informational block to stderr — verification findings and the
/// like, printed beside a command's own output without claiming to be an
/// error.
pub fn note(message: impl Display) {
    eprintln!("{message}");
}

/// Renders a check verdict's error list the way a person reads it:
/// `<path>: <n> error(s)`, then each on its own indented line. The one
/// place `check`, `run` and `new` format the same block, so a reader
/// sees one shape wherever a workflow's errors surface.
pub fn error_block(path: &Path, items: &[impl Display]) -> String {
    let mut block = format!("{}: {} error(s)", path.display(), items.len());
    for item in items {
        block.push_str(&format!("\n  {item}"));
    }
    block
}
