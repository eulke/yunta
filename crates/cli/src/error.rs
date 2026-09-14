//! The one error a command hands back and the one place its text reaches
//! a person. Every subcommand returns [`Result<Outcome, CliError>`]; `main`
//! prints a [`CliError`] once, on stderr, and maps the exit code. A
//! command that ran but reports a failing verdict (an invalid workflow, a
//! test case that failed) returns [`Outcome::Reported`] instead — its
//! detail is already on the command's own output, and no error banner
//! belongs on top of it.

use std::fmt::Display;
use yunta_core::RunId;

/// How a subcommand came back when nothing stopped it from running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The command did its job; exit 0.
    Success,
    /// The command ran and reports a failing verdict — the detail is
    /// already on its output. Exit non-zero, no error banner.
    Reported,
    /// The command's whole answer is its exit code, because the process
    /// that ran it reads one: the fence hook, whose calling CLI takes
    /// `2` as a refusal and `0` as consent.
    Code(u8),
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

    /// A run this binary has no record of, wherever it looked. One
    /// sentence, because a person who mistyped an id gets the same
    /// answer whichever command they typed it into.
    #[error("no run `{id}` under {}", .roots.iter().map(|root| root.display().to_string()).collect::<Vec<_>>().join(" or "))]
    RunNotFound {
        id: RunId,
        roots: Vec<std::path::PathBuf>,
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
    DetachedResume(#[from] crate::commands::DetachedResumeError),

    #[error(transparent)]
    ResolveGate(#[from] yunta_engine::ResolveGateError),

    /// A decision was put to a run that is not parked at one. The
    /// engine's sentence says what is true of the run; which command
    /// shows a reader where it actually is is this border's word, so it
    /// is added here — once, for the command and the control plane
    /// alike.
    #[error("{refusal} — `{}` shows where it is", crate::commands::advice::status(.run_id))]
    NotPaused {
        run_id: RunId,
        #[source]
        refusal: yunta_engine::ResolveGateError,
    },

    /// A decision was put to a run whose pause reconstructs no menu.
    /// Those pauses are settled where they were raised — a budget, a
    /// scope, an answers file, a review on a forge — and the run handed
    /// back, which is what the advice names.
    #[error("{refusal} — settle it where it was raised, then `{}`", crate::commands::advice::resume(.run_id))]
    NoMenu {
        run_id: RunId,
        #[source]
        refusal: yunta_engine::ResolveGateError,
    },

    /// A mock fixture a `yunta test` case or `yunta run --fixture`
    /// named does not parse. The path leads the sentence, because a
    /// person running several cases needs to know which fixture broke
    /// before they need to know how.
    #[error("fixture `{}`: {source}", .path.display())]
    FixtureRefused {
        path: std::path::PathBuf,
        #[source]
        source: yunta_adapters::FixtureError,
    },

    /// A decision was recorded and the run could not be handed back to
    /// a detached `yunta resume`. The sentence leads with what did
    /// happen, because a reader who takes this for a refusal answers
    /// the same gate twice.
    #[error("decision recorded, but {source}")]
    GateRecordedNotResumed {
        #[source]
        source: crate::commands::DetachedResumeError,
    },

    /// A document kind this binary does not publish. The sentence
    /// lists the kinds that exist, so `yunta schema` and the
    /// `document_shape` tool answer the same mistake the same way.
    #[error(transparent)]
    UnknownArtifactKind(#[from] yunta_core::UnknownArtifactKind),

    /// A value that has to be an identifier and is not — a run id, an
    /// adapter, a mode, a gate option, a responder — wherever one is
    /// read off an argument.
    #[error(transparent)]
    InvalidId(#[from] yunta_core::InvalidId),

    #[error(transparent)]
    Worktree(#[from] yunta_engine::WorktreeError),

    #[error(transparent)]
    FrozenPaths(#[from] yunta_core::RelativeRootError),

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

    /// An engine refusal to record a decision, in this border's
    /// vocabulary.
    ///
    /// Two of them describe the state the run is in rather than
    /// anything about the request, and a reader told the run cannot be
    /// answered wants to know what to do instead. Which command does
    /// that is the CLI's word, not the engine's, so it is said here —
    /// and every other refusal already names what to change (an option
    /// off the menu lists the ones that are on it) and passes through
    /// untouched.
    pub fn gate_refused(run_id: &RunId, refusal: yunta_engine::ResolveGateError) -> Self {
        let run_id = run_id.clone();
        match refusal {
            yunta_engine::ResolveGateError::NotPaused => CliError::NotPaused { run_id, refusal },
            yunta_engine::ResolveGateError::NothingToResolve => {
                CliError::NoMenu { run_id, refusal }
            }
            yunta_engine::ResolveGateError::UnknownOption { .. }
            | yunta_engine::ResolveGateError::Storage(_) => refusal.into(),
        }
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
